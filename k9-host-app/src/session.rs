// INPUT:  serde_json（hook 原始事件）、std::time、std::collections::HashMap
// OUTPUT: AgentStatus、SessionState、HookEvent、reduce()、sweep_sessions()、is_activity_event()、is_tool_completion_event() — 会话状态机纯 reducer + 事件解析 + 看门狗 sweep
// POS:    AI 审批面板核心逻辑 — 5 态会话状态机（照 CodeIsland 蓝本迁移表），无 GPUI 依赖可单测

use std::collections::HashMap;
use std::time::SystemTime;

/// 会话状态（5 态，照蓝本 SessionSnapshot）
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AgentStatus {
    #[default]
    Idle,
    Processing,
    Running,
    WaitingApproval,
    WaitingQuestion,
}

impl AgentStatus {
    /// 刘海条会话优先级：waitingApproval > waitingQuestion > running > processing > idle
    pub fn priority(self) -> u8 {
        match self {
            AgentStatus::WaitingApproval => 5,
            AgentStatus::WaitingQuestion => 4,
            AgentStatus::Running => 3,
            AgentStatus::Processing => 2,
            AgentStatus::Idle => 0,
        }
    }

    pub fn is_waiting(self) -> bool {
        matches!(
            self,
            AgentStatus::WaitingApproval | AgentStatus::WaitingQuestion
        )
    }
}

/// 单个 AI CLI 会话的最新状态（刘海条数据源）
#[derive(Debug, Clone)]
pub struct SessionState {
    /// 与 sessions 表的 key 一致（冗余存储，便于展示/调试）
    #[allow(dead_code)]
    pub session_id: String,
    pub source: String,
    pub status: AgentStatus,
    pub current_tool: Option<String>,
    pub last_user_msg: Option<String>,
    pub last_activity: SystemTime,
}

impl SessionState {
    pub fn new(session_id: String, source: String) -> Self {
        Self {
            session_id,
            source,
            status: AgentStatus::Idle,
            current_tool: None,
            last_user_msg: None,
            last_activity: SystemTime::now(),
        }
    }
}

/// 从 hook JSON 解析出的归一化事件（reducer 的输入）
#[derive(Debug, Clone)]
pub struct HookEvent {
    pub event_name: String,
    pub session_id: String,
    pub source: String,
    pub tool_name: Option<String>,
    /// UserPromptSubmit 的 prompt / Notification 的 message
    pub message: Option<String>,
    /// Notification 是否携带 question（阻塞提问）
    pub has_question: bool,
    /// 工具调用关联 id（Phase 4：同 id 完成事件释放 pending / 重复请求去重）
    pub tool_use_id: Option<String>,
}

/// UserPromptSubmit 的 prompt 提取：字符串原样；content-part 数组
/// （Kimi Code 特例：`[{"type":"text","text":"..."}]`）拼接 text 部分（照蓝本空串拼接）。
fn prompt_text(v: &serde_json::Value) -> Option<String> {
    match v {
        serde_json::Value::String(s) => Some(s.clone()),
        serde_json::Value::Array(parts) => {
            let text: String = parts
                .iter()
                .filter(|p| p.get("type").and_then(|t| t.as_str()) == Some("text"))
                .filter_map(|p| p.get("text").and_then(|t| t.as_str()))
                .collect();
            if text.is_empty() {
                None
            } else {
                Some(text)
            }
        }
        _ => None,
    }
}

impl HookEvent {
    /// 从原始 hook JSON 构造（字段名照蓝本：hook_event_name / session_id / _source / tool_name）
    pub fn from_json(v: &serde_json::Value) -> Self {
        let event_name = v
            .get("hook_event_name")
            .and_then(|x| x.as_str())
            .unwrap_or("")
            .to_string();
        let session_id = v
            .get("session_id")
            .and_then(|x| x.as_str())
            .filter(|s| !s.is_empty())
            .unwrap_or("default")
            .to_string();
        let source = v
            .get("_source")
            .and_then(|x| x.as_str())
            .filter(|s| !s.is_empty())
            .unwrap_or("claude")
            .to_string();
        let tool_name = v
            .get("tool_name")
            .and_then(|x| x.as_str())
            .map(|s| s.to_string());
        let message = v.get("prompt").and_then(prompt_text).or_else(|| {
            v.get("message")
                .and_then(|x| x.as_str())
                .map(|s| s.to_string())
        });
        let has_question = v.get("question").and_then(|x| x.as_str()).is_some();
        let tool_use_id = v
            .get("tool_use_id")
            .and_then(|x| x.as_str())
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string());
        Self {
            event_name,
            session_id,
            source,
            tool_name,
            message,
            has_question,
            tool_use_id,
        }
    }
}

/// reduce 结果：会话保留还是移除（SessionEnd）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReduceOutcome {
    Keep,
    Remove,
}

/// 纯 reducer：按事件迁移会话状态（迁移表照蓝本，v1 不含 subagent 细节）。
///
/// waiting 粘性：WaitingApproval/WaitingQuestion 状态下，普通活动事件
/// （PreToolUse/PostToolUse 等）不改变状态；UserPromptSubmit/Stop/PermissionRequest
/// 等权威事件仍会覆盖。
pub fn reduce(session: &mut SessionState, event: &HookEvent) -> ReduceOutcome {
    session.last_activity = SystemTime::now();
    if !event.source.is_empty() {
        session.source = event.source.clone();
    }
    let is_waiting = session.status.is_waiting();

    match event.event_name.as_str() {
        "SessionStart" => {
            // 新会话重置（保留 session_id / source）
            session.status = AgentStatus::Idle;
            session.current_tool = None;
            session.last_user_msg = None;
        }
        "SessionEnd" => return ReduceOutcome::Remove,
        "UserPromptSubmit" => {
            session.status = AgentStatus::Processing;
            session.current_tool = None;
            if let Some(msg) = &event.message {
                session.last_user_msg = Some(msg.clone());
            }
        }
        "PreToolUse" => {
            if !is_waiting {
                session.status = AgentStatus::Running;
                session.current_tool = event.tool_name.clone();
            }
        }
        "PostToolUse" | "PostToolUseFailure" => {
            if !is_waiting {
                session.status = AgentStatus::Processing;
                session.current_tool = None;
            }
        }
        "PermissionRequest" => {
            session.status = if event.tool_name.as_deref() == Some("AskUserQuestion") {
                AgentStatus::WaitingQuestion
            } else {
                AgentStatus::WaitingApproval
            };
            if event.tool_name.is_some() {
                session.current_tool = event.tool_name.clone();
            }
        }
        "Notification" => {
            // 仅携带 question 的 Notification 是阻塞提问（蓝本 QuestionPayload 规则）
            if event.has_question {
                session.status = AgentStatus::WaitingQuestion;
            }
        }
        "Stop" => {
            session.status = AgentStatus::Idle;
            session.current_tool = None;
        }
        "PreCompact" => {
            if !is_waiting {
                session.status = AgentStatus::Processing;
            }
        }
        _ => {}
    }

    ReduceOutcome::Keep
}

/// 「agent 已继续推进」活动事件（照蓝本 resolveOrphanPermissionsOnActivity）：
/// waiting 会话收到这些事件，说明用户可能已在终端批准，无 tool_use_id 的
/// pending 请求可视为已批准（allow 释放）。新 PermissionRequest / 带 question 的
/// Notification 走自己的入队路径，不在此列。
pub fn is_activity_event(name: &str) -> bool {
    matches!(
        name,
        "PreToolUse" | "PostToolUse" | "PostToolUseFailure" | "Stop" | "UserPromptSubmit"
    )
}

/// 工具完成事件（照蓝本 resolveToolUseIfCompleted）：携带 tool_use_id 时，
/// 同 id 的 pending 请求已被 agent 跳过/完成，deny 释放以免拖住 UI。
pub fn is_tool_completion_event(name: &str) -> bool {
    matches!(
        name,
        "PostToolUse" | "PostToolUseFailure" | "PermissionDenied"
    )
}

/// waiting 会话无活动强制回 Idle 的阈值（照蓝本 300s）
pub const WAITING_FORCE_IDLE_SECS: u64 = 300;

/// Idle 会话无活动被移除的阈值（照蓝本 hook-only 会话 10 分钟默认）
pub const IDLE_REMOVE_SECS: u64 = 600;

/// 看门狗 sweep 动作
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SweepAction {
    /// waiting 会话超时强制回 Idle（其 pending 请求应由调用方 deny 释放）
    ForceIdle,
    /// Idle 会话超时移除（其残留 pending 请求应由调用方 deny 释放）
    Remove,
}

/// 看门狗 sweep（照蓝本 cleanup timer 第 2/4 节，v1 无进程监控只保留时间维度）：
/// - waitingApproval/waitingQuestion 会话 300s 无活动 → 强制 Idle + 清工具；
/// - Idle 会话 600s 无活动 → 移除。
///
/// 直接原地修改 sessions，返回受影响 (session_id, action) 列表，
/// 调用方据此 deny 释放关联 pending 请求（蓝本 removeSession/drainPermissions 语义）。
pub fn sweep_sessions(
    sessions: &mut HashMap<String, SessionState>,
    now: SystemTime,
) -> Vec<(String, SweepAction)> {
    let mut actions = Vec::new();
    let mut remove_ids = Vec::new();
    for (id, sess) in sessions.iter_mut() {
        let elapsed = now
            .duration_since(sess.last_activity)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        if sess.status.is_waiting() && elapsed > WAITING_FORCE_IDLE_SECS {
            sess.status = AgentStatus::Idle;
            sess.current_tool = None;
            actions.push((id.clone(), SweepAction::ForceIdle));
        } else if sess.status == AgentStatus::Idle && elapsed > IDLE_REMOVE_SECS {
            remove_ids.push(id.clone());
        }
    }
    for id in remove_ids {
        sessions.remove(&id);
        actions.push((id, SweepAction::Remove));
    }
    actions
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ev(name: &str) -> HookEvent {
        HookEvent {
            event_name: name.into(),
            session_id: "s1".into(),
            source: "claude".into(),
            tool_name: None,
            message: None,
            has_question: false,
            tool_use_id: None,
        }
    }

    fn session() -> SessionState {
        SessionState::new("s1".into(), "claude".into())
    }

    #[test]
    fn transition_table() {
        let mut s = session();

        // UserPromptSubmit → Processing + last_user_msg
        let mut e = ev("UserPromptSubmit");
        e.message = Some("fix the bug".into());
        assert_eq!(reduce(&mut s, &e), ReduceOutcome::Keep);
        assert_eq!(s.status, AgentStatus::Processing);
        assert_eq!(s.last_user_msg.as_deref(), Some("fix the bug"));

        // PreToolUse → Running + current_tool
        let mut e = ev("PreToolUse");
        e.tool_name = Some("Bash".into());
        reduce(&mut s, &e);
        assert_eq!(s.status, AgentStatus::Running);
        assert_eq!(s.current_tool.as_deref(), Some("Bash"));

        // PostToolUse → Processing + 清工具
        reduce(&mut s, &ev("PostToolUse"));
        assert_eq!(s.status, AgentStatus::Processing);
        assert_eq!(s.current_tool, None);

        // PreToolUse → Running，PostToolUseFailure → Processing
        let mut e = ev("PreToolUse");
        e.tool_name = Some("Edit".into());
        reduce(&mut s, &e);
        assert_eq!(s.status, AgentStatus::Running);
        reduce(&mut s, &ev("PostToolUseFailure"));
        assert_eq!(s.status, AgentStatus::Processing);

        // PermissionRequest → WaitingApproval + current_tool
        let mut e = ev("PermissionRequest");
        e.tool_name = Some("Bash".into());
        reduce(&mut s, &e);
        assert_eq!(s.status, AgentStatus::WaitingApproval);
        assert_eq!(s.current_tool.as_deref(), Some("Bash"));

        // Stop → Idle
        reduce(&mut s, &ev("Stop"));
        assert_eq!(s.status, AgentStatus::Idle);
        assert_eq!(s.current_tool, None);
    }

    #[test]
    fn session_start_resets_and_end_removes() {
        let mut s = session();
        s.status = AgentStatus::Running;
        s.current_tool = Some("Bash".into());
        s.last_user_msg = Some("hi".into());

        reduce(&mut s, &ev("SessionStart"));
        assert_eq!(s.status, AgentStatus::Idle);
        assert_eq!(s.current_tool, None);
        assert_eq!(s.last_user_msg, None);

        assert_eq!(reduce(&mut s, &ev("SessionEnd")), ReduceOutcome::Remove);
    }

    #[test]
    fn notification_question_waits() {
        let mut s = session();
        let mut e = ev("Notification");
        e.has_question = true;
        reduce(&mut s, &e);
        assert_eq!(s.status, AgentStatus::WaitingQuestion);

        // 普通 Notification 不改变状态
        let mut s = session();
        reduce(&mut s, &ev("Notification"));
        assert_eq!(s.status, AgentStatus::Idle);
    }

    #[test]
    fn ask_user_question_uses_question_state() {
        let mut s = session();
        let mut e = ev("PermissionRequest");
        e.tool_name = Some("AskUserQuestion".into());
        reduce(&mut s, &e);
        assert_eq!(s.status, AgentStatus::WaitingQuestion);
        assert_eq!(s.current_tool.as_deref(), Some("AskUserQuestion"));
    }

    #[test]
    fn waiting_stickiness() {
        for waiting in [AgentStatus::WaitingApproval, AgentStatus::WaitingQuestion] {
            let mut s = session();
            s.status = waiting;

            // 普通活动事件不覆盖 waiting
            let mut e = ev("PreToolUse");
            e.tool_name = Some("Read".into());
            reduce(&mut s, &e);
            assert_eq!(s.status, waiting);
            reduce(&mut s, &ev("PostToolUse"));
            assert_eq!(s.status, waiting);
            reduce(&mut s, &ev("PreCompact"));
            assert_eq!(s.status, waiting);

            // 权威事件仍可覆盖
            reduce(&mut s, &ev("UserPromptSubmit"));
            assert_eq!(s.status, AgentStatus::Processing);

            let mut s = session();
            s.status = waiting;
            reduce(&mut s, &ev("Stop"));
            assert_eq!(s.status, AgentStatus::Idle);
        }
    }

    #[test]
    fn status_priority_order() {
        assert!(AgentStatus::WaitingApproval.priority() > AgentStatus::WaitingQuestion.priority());
        assert!(AgentStatus::WaitingQuestion.priority() > AgentStatus::Running.priority());
        assert!(AgentStatus::Running.priority() > AgentStatus::Processing.priority());
        assert!(AgentStatus::Processing.priority() > AgentStatus::Idle.priority());
    }

    #[test]
    fn from_json_parses_blueprint_fields() {
        let v = serde_json::json!({
            "session_id": "s1",
            "hook_event_name": "PermissionRequest",
            "tool_name": "Bash",
            "tool_input": {"command": "ls"},
            "_source": "claude"
        });
        let e = HookEvent::from_json(&v);
        assert_eq!(e.event_name, "PermissionRequest");
        assert_eq!(e.session_id, "s1");
        assert_eq!(e.source, "claude");
        assert_eq!(e.tool_name.as_deref(), Some("Bash"));
        assert!(!e.has_question);

        // 缺 session_id 兜底 default，缺 _source 兜底 claude
        let v = serde_json::json!({"hook_event_name": "Stop"});
        let e = HookEvent::from_json(&v);
        assert_eq!(e.session_id, "default");
        assert_eq!(e.source, "claude");

        // question 检测
        let v = serde_json::json!({"hook_event_name": "Notification", "question": "继续吗？"});
        assert!(HookEvent::from_json(&v).has_question);
    }

    #[test]
    fn activity_and_completion_predicates() {
        for name in [
            "PreToolUse",
            "PostToolUse",
            "PostToolUseFailure",
            "Stop",
            "UserPromptSubmit",
        ] {
            assert!(is_activity_event(name), "{name}");
        }
        for name in [
            "PermissionRequest",
            "Notification",
            "SessionStart",
            "SessionEnd",
            "PreCompact",
        ] {
            assert!(!is_activity_event(name), "{name}");
        }
        for name in ["PostToolUse", "PostToolUseFailure", "PermissionDenied"] {
            assert!(is_tool_completion_event(name), "{name}");
        }
        for name in [
            "PreToolUse",
            "Stop",
            "UserPromptSubmit",
            "PermissionRequest",
        ] {
            assert!(!is_tool_completion_event(name), "{name}");
        }
    }

    #[test]
    fn tool_use_id_parsed_when_present() {
        let v = serde_json::json!({
            "hook_event_name": "PostToolUse",
            "session_id": "s1",
            "tool_use_id": "toolu_123"
        });
        assert_eq!(
            HookEvent::from_json(&v).tool_use_id.as_deref(),
            Some("toolu_123")
        );

        // 空串视为缺失
        let v = serde_json::json!({"hook_event_name": "Stop", "tool_use_id": ""});
        assert_eq!(HookEvent::from_json(&v).tool_use_id, None);

        let v = serde_json::json!({"hook_event_name": "Stop"});
        assert_eq!(HookEvent::from_json(&v).tool_use_id, None);
    }

    #[test]
    fn sweep_force_idle_and_remove() {
        use std::time::Duration;
        let now = SystemTime::now();
        let mut sessions = HashMap::new();

        // waiting 301s 无活动 → ForceIdle
        let mut waiting = SessionState::new("waiting".into(), "claude".into());
        waiting.status = AgentStatus::WaitingApproval;
        waiting.current_tool = Some("Bash".into());
        waiting.last_activity = now - Duration::from_secs(WAITING_FORCE_IDLE_SECS + 1);
        sessions.insert("waiting".into(), waiting);

        // waiting 但仍在阈值内 → 不动
        let mut fresh_waiting = SessionState::new("fresh".into(), "claude".into());
        fresh_waiting.status = AgentStatus::WaitingQuestion;
        fresh_waiting.last_activity = now - Duration::from_secs(WAITING_FORCE_IDLE_SECS - 10);
        sessions.insert("fresh".into(), fresh_waiting);

        // idle 601s 无活动 → Remove
        let mut stale_idle = SessionState::new("stale".into(), "codex".into());
        stale_idle.last_activity = now - Duration::from_secs(IDLE_REMOVE_SECS + 1);
        sessions.insert("stale".into(), stale_idle);

        // idle 但新鲜 → 保留
        let fresh_idle = SessionState::new("idle".into(), "codex".into());
        sessions.insert("idle".into(), fresh_idle);

        // running 超时不处理（v1 只 sweep waiting/idle）
        let mut running = SessionState::new("running".into(), "claude".into());
        running.status = AgentStatus::Running;
        running.last_activity = now - Duration::from_secs(IDLE_REMOVE_SECS + 100);
        sessions.insert("running".into(), running);

        let mut actions = sweep_sessions(&mut sessions, now);
        actions.sort_by(|a, b| a.0.cmp(&b.0));
        assert_eq!(
            actions,
            vec![
                ("stale".to_string(), SweepAction::Remove),
                ("waiting".to_string(), SweepAction::ForceIdle),
            ]
        );
        assert_eq!(sessions["waiting"].status, AgentStatus::Idle);
        assert_eq!(sessions["waiting"].current_tool, None);
        assert_eq!(sessions["fresh"].status, AgentStatus::WaitingQuestion);
        assert!(sessions.contains_key("idle"));
        assert!(!sessions.contains_key("stale"));
        assert_eq!(sessions["running"].status, AgentStatus::Running);

        // ForceIdle 不刷新 last_activity：idle 计时仍基于原时间，累计超 600s 后即移除
        let later = now + Duration::from_secs(IDLE_REMOVE_SECS);
        let actions = sweep_sessions(&mut sessions, later);
        assert!(actions.contains(&("waiting".to_string(), SweepAction::Remove)));
        assert!(!sessions.contains_key("waiting"));
    }

    #[test]
    fn kimi_prompt_content_part_array_joined() {
        // Kimi Code 特例：prompt 是 content-part 数组，拼接 text 部分
        let v = serde_json::json!({
            "hook_event_name": "UserPromptSubmit",
            "_source": "kimi",
            "prompt": [{"type": "text", "text": "Optimize the node parameter menu"}]
        });
        let e = HookEvent::from_json(&v);
        assert_eq!(
            e.message.as_deref(),
            Some("Optimize the node parameter menu")
        );

        // 多 text part 空串拼接，非 text part 跳过（照蓝本）
        let v = serde_json::json!({
            "hook_event_name": "UserPromptSubmit",
            "_source": "kimi",
            "prompt": [
                {"type": "text", "text": "Hello "},
                {"type": "text", "text": "world"},
                {"type": "image", "url": "https://example.com/a.png"}
            ]
        });
        assert_eq!(
            HookEvent::from_json(&v).message.as_deref(),
            Some("Hello world")
        );

        // 字符串 prompt 原样
        let v = serde_json::json!({"hook_event_name": "UserPromptSubmit", "prompt": "plain"});
        assert_eq!(HookEvent::from_json(&v).message.as_deref(), Some("plain"));

        // 数组无 text part → None，回退 message 字段
        let v = serde_json::json!({
            "hook_event_name": "Notification",
            "prompt": [{"type": "image", "url": "x"}],
            "message": "fallback"
        });
        assert_eq!(
            HookEvent::from_json(&v).message.as_deref(),
            Some("fallback")
        );
    }
}
