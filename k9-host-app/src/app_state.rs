// INPUT:  gpui, shared-datachannel-proto (DeviceCapabilities, PadConfig, DialogResultCode), bridge (HostCommand), session (SessionState, HookEvent, 活动/完成事件谓词), test_state
// OUTPUT: AppState (Global), ConnectionStatus, AppEvent, Page, SlotContent, PendingPermission/PendingPrompt、问题选项推进、审批队列维护函数
// POS:    应用状态定义 — GPUI Global 状态容器，用于跨运行时同步 BLE 连接状态、设备信息、弹窗结果与 slot 显示数据（刘海浮窗数据源），含测试页面状态与 AI 会话/审批队列；
//         审批队列 drain 纯函数镜像 tokio 侧 pending drain 规则（GPUI 出队与 oneshot 完成同源同事件）

use std::collections::{HashMap, VecDeque};
use std::sync::mpsc;

use gpui::Global;
use k9_datachannel_proto::{DeviceCapabilities, DialogResultCode, PadConfig};

use crate::bridge::HostCommand;
use crate::session::{self, HookEvent, SessionState};
use crate::test_state::TestState;

/// BLE connection status.
#[derive(Debug, Clone)]
pub enum ConnectionStatus {
    Disconnected,
    Connecting,
    Connected,
    Error(String),
}

impl Default for ConnectionStatus {
    fn default() -> Self {
        Self::Disconnected
    }
}

/// Active page in the application.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Page {
    #[default]
    Home,
    Test,
}

/// 单个显示 slot 的最新内容（与设备上屏数据一致，刘海浮窗直接消费）
#[derive(Debug, Clone)]
pub enum SlotContent {
    Text(String),
    Numeric(i32),
    Progress(u8),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PromptOption {
    pub label: String,
    pub description: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChoiceQuestion {
    /// `updatedInput.answers` 使用的稳定 key；重复问题由解析层添加 `_2` 后缀。
    pub key: String,
    pub prompt: String,
    pub options: Vec<PromptOption>,
    pub multi_select: bool,
    pub selected: Vec<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PendingPrompt {
    Approval,
    Questions {
        questions: Vec<ChoiceQuestion>,
        current: usize,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub enum QuestionProgress {
    Updated,
    Complete(serde_json::Map<String, serde_json::Value>),
}

impl PendingPrompt {
    pub fn current_question(&self) -> Option<(&ChoiceQuestion, usize, usize)> {
        let Self::Questions { questions, current } = self else {
            return None;
        };
        questions
            .get(*current)
            .map(|question| (question, *current, questions.len()))
    }

    /// 单选立即进入下一题；多选只切换勾选，等待显式 submit。
    pub fn select_option(&mut self, option_index: usize) -> Option<QuestionProgress> {
        let Self::Questions { questions, current } = self else {
            return None;
        };
        let question = questions.get_mut(*current)?;
        if option_index >= question.options.len() {
            return None;
        }
        if question.multi_select {
            if let Some(position) = question
                .selected
                .iter()
                .position(|selected| *selected == option_index)
            {
                question.selected.remove(position);
            } else {
                question.selected.push(option_index);
                question.selected.sort_unstable();
            }
            Some(QuestionProgress::Updated)
        } else {
            question.selected.clear();
            question.selected.push(option_index);
            self.advance_question()
        }
    }

    pub fn submit_current(&mut self) -> Option<QuestionProgress> {
        let Self::Questions { questions, current } = self else {
            return None;
        };
        let question = questions.get(*current)?;
        if !question.multi_select || question.selected.is_empty() {
            return None;
        }
        self.advance_question()
    }

    fn advance_question(&mut self) -> Option<QuestionProgress> {
        let Self::Questions { questions, current } = self else {
            return None;
        };
        if *current + 1 < questions.len() {
            *current += 1;
            Some(QuestionProgress::Updated)
        } else {
            Some(QuestionProgress::Complete(question_answers(questions)))
        }
    }
}

fn question_answers(questions: &[ChoiceQuestion]) -> serde_json::Map<String, serde_json::Value> {
    questions
        .iter()
        .map(|question| {
            let labels: Vec<String> = question
                .selected
                .iter()
                .filter_map(|index| question.options.get(*index))
                .map(|option| option.label.clone())
                .collect();
            let value = if question.multi_select {
                serde_json::Value::Array(
                    labels.into_iter().map(serde_json::Value::String).collect(),
                )
            } else {
                serde_json::Value::String(labels.into_iter().next().unwrap_or_default())
            };
            (question.key.clone(), value)
        })
        .collect()
}

/// 待处理的权限/问题请求（全局 FIFO 队列元素，队首渲染卡片）。
#[derive(Debug, Clone)]
pub struct PendingPermission {
    pub req_id: u64,
    pub session_id: String,
    pub source: String,
    pub tool_name: String,
    pub summary: String,
    pub prompt: PendingPrompt,
    /// Phase 4「同 tool_use_id 去重/终端批准启发式」用：与 tokio 侧 PendingEntry 同步出队
    pub tool_use_id: Option<String>,
}

/// 活动事件后审批队列出队（镜像 tokio 侧 drain_pending_on_activity 的移除规则，
/// 决策本身由 tokio 侧完成 oneshot 时给出，这里只同步队列）：
/// - 活动事件 → 移除该 session 无 tool_use_id 的条目（tokio 侧已 allow）；
/// - 完成事件携带 tool_use_id → 移除同 id 条目（tokio 侧已 deny）。
/// 返回被移除的 req_ids。
pub fn drain_queue_on_activity(
    queue: &mut VecDeque<PendingPermission>,
    ev: &HookEvent,
) -> Vec<u64> {
    let is_activity = session::is_activity_event(&ev.event_name);
    let is_completion = session::is_tool_completion_event(&ev.event_name);
    if !is_activity && !is_completion {
        return Vec::new();
    }
    let mut removed = Vec::new();
    queue.retain(|p| {
        let orphan_allow = is_activity && p.session_id == ev.session_id && p.tool_use_id.is_none();
        let tracked_deny =
            is_completion && ev.tool_use_id.is_some() && p.tool_use_id == ev.tool_use_id;
        if orphan_allow || tracked_deny {
            removed.push(p.req_id);
            false
        } else {
            true
        }
    });
    removed
}

/// SessionEnd / 会话被移除：出队该 session 全部 pending（tokio 侧已 deny），
/// 返回被移除的 req_ids。
pub fn remove_session_from_queue(
    queue: &mut VecDeque<PendingPermission>,
    session_id: &str,
) -> Vec<u64> {
    let mut removed = Vec::new();
    queue.retain(|p| {
        if p.session_id == session_id {
            removed.push(p.req_id);
            false
        } else {
            true
        }
    });
    removed
}

/// 同 tool_use_id 重复 PermissionRequest（镜像 tokio 侧 deny_duplicate_tool_use）：
/// 队列已有同非空 tool_use_id 条目时原位替换（保持卡片位置不抖动）并返回 true，
/// 否则返回 false（调用方照常 push_back）。
pub fn replace_duplicate_in_queue(
    queue: &mut VecDeque<PendingPermission>,
    req: PendingPermission,
) -> bool {
    let Some(tuid) = &req.tool_use_id else {
        return false;
    };
    if let Some(slot) = queue
        .iter_mut()
        .find(|p| p.tool_use_id.as_deref() == Some(tuid.as_str()))
    {
        *slot = req;
        true
    } else {
        false
    }
}

/// Global application state shared between tokio bridge and GPUI UI.
#[derive(Default)]
pub struct AppState {
    pub connection: ConnectionStatus,
    pub device_caps: Option<DeviceCapabilities>,
    pub pad_config: Option<PadConfig>,
    pub page: Page,
    pub test_state: TestState,
    /// GPUI→tokio command channel (set once at startup by main).
    pub host_command_tx: Option<mpsc::Sender<HostCommand>>,
    /// Whether the connected device's protocol version supports dialogs (>= 2).
    pub dialog_supported: bool,
    /// Last dialog result received from the device, if any.
    pub last_dialog_result: Option<(u8, DialogResultCode)>,
    /// Total number of dialog results received (monotonic counter).
    pub dialog_result_count: u32,
    /// 各显示 slot 的最新内容（None = 无数据/已清除），镜像推送到设备的数据
    pub slots: [Option<SlotContent>; 8],
    /// AI CLI 会话表（session_id → 状态），reducer 驱动，刘海条会话区数据源
    pub sessions: HashMap<String, SessionState>,
    /// 权限请求全局 FIFO 队列（队首渲染审批卡）
    pub permission_queue: VecDeque<PendingPermission>,
    /// 刘海面板端部样式（notch/capsule），调试面板可实时切换并持久化
    pub notch_style: crate::notch_shape::EndStyle,
    /// 响应式宽度基线调参值（1.0–3.0，1.8 = 1.0× 基线），调试面板可实时调节
    pub widen_factor: f32,
    /// 紧凑态顶部反角/底部圆角（32pt 行高基准，默认 6/14；下拉时过渡到 12/22）
    pub notch_top_r: f32,
    pub notch_bot_r: f32,
}

impl Global for AppState {}

/// Events sent from the tokio thread to the GPUI bridge loop.
pub enum AppEvent {
    ConnectionChanged(ConnectionStatus),
    DeviceCaps(DeviceCapabilities),
    PadConfigUpdated(PadConfig),
    DialogResult {
        id: u8,
        result: DialogResultCode,
    },
    /// Provider 推送成功后的 slot 数据镜像（content 为 None 表示清除）
    SlotUpdate {
        slot: u8,
        content: Option<SlotContent>,
    },
    /// 普通 hook 事件（驱动 session reducer）
    HookEvent(HookEvent),
    /// 阻塞权限/提问请求（reducer 置 WaitingApproval/WaitingQuestion + 入全局请求队列）
    PermissionRequest {
        req_id: u64,
        session_id: String,
        source: String,
        tool_name: String,
        summary: String,
        prompt: PendingPrompt,
        tool_use_id: Option<String>,
    },
    /// 设备渠道已应答某权限请求（GPUI 出队 + 会话回 Processing，幂等）
    PermissionAnswered {
        req_id: u64,
    },
    /// 看门狗 tick（tokio 侧 60s interval 触发）：sweep waiting/idle 超时会话
    WatchdogSweep,
    Shutdown,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn req(req_id: u64, session_id: &str, tool_use_id: Option<&str>) -> PendingPermission {
        PendingPermission {
            req_id,
            session_id: session_id.into(),
            source: "claude".into(),
            tool_name: "Bash".into(),
            summary: "ls".into(),
            prompt: PendingPrompt::Approval,
            tool_use_id: tool_use_id.map(|s| s.into()),
        }
    }

    fn ev(name: &str, session_id: &str, tool_use_id: Option<&str>) -> HookEvent {
        HookEvent {
            event_name: name.into(),
            session_id: session_id.into(),
            source: "claude".into(),
            tool_name: None,
            message: None,
            has_question: false,
            tool_use_id: tool_use_id.map(|s| s.into()),
        }
    }

    fn queue_ids(queue: &VecDeque<PendingPermission>) -> Vec<u64> {
        queue.iter().map(|p| p.req_id).collect()
    }

    #[test]
    fn drain_queue_on_activity_mirrors_tokio_rules() {
        let mut q = VecDeque::from(vec![
            req(1, "s1", None),
            req(2, "s1", Some("toolu_a")),
            req(3, "s2", None),
        ]);

        // 活动事件：只移除 s1 的 orphan
        let removed = drain_queue_on_activity(&mut q, &ev("Stop", "s1", None));
        assert_eq!(removed, vec![1]);
        assert_eq!(queue_ids(&q), vec![2, 3]);

        // 完成事件：移除同 tool_use_id 条目
        let removed = drain_queue_on_activity(&mut q, &ev("PostToolUse", "s1", Some("toolu_a")));
        assert_eq!(removed, vec![2]);
        assert_eq!(queue_ids(&q), vec![3]);

        // 非活动事件不动队列
        let removed = drain_queue_on_activity(&mut q, &ev("SessionStart", "s2", None));
        assert!(removed.is_empty());
        assert_eq!(queue_ids(&q), vec![3]);

        // 完成事件不带 tool_use_id：PostToolUse 是活动事件，orphan 仍出队
        let removed = drain_queue_on_activity(&mut q, &ev("PostToolUse", "s2", None));
        assert_eq!(removed, vec![3]);
        assert!(q.is_empty());
    }

    #[test]
    fn remove_session_from_queue_drains_all() {
        let mut q = VecDeque::from(vec![
            req(1, "s1", None),
            req(2, "s2", Some("toolu_a")),
            req(3, "s1", Some("toolu_b")),
        ]);
        let removed = remove_session_from_queue(&mut q, "s1");
        assert_eq!(removed, vec![1, 3]);
        assert_eq!(queue_ids(&q), vec![2]);
    }

    #[test]
    fn replace_duplicate_in_queue_keeps_position() {
        let mut q = VecDeque::from(vec![
            req(1, "s1", Some("toolu_a")),
            req(2, "s1", None),
            req(3, "s2", Some("toolu_b")),
        ]);

        // 同 tool_use_id：原位替换（位置 0 不动），req_id/摘要更新
        let new = req(9, "s1", Some("toolu_a"));
        assert!(replace_duplicate_in_queue(&mut q, new));
        assert_eq!(queue_ids(&q), vec![9, 2, 3]);

        // 无重复 → false，调用方 push_back
        let new = req(10, "s1", Some("toolu_c"));
        assert!(!replace_duplicate_in_queue(&mut q, new));

        // 无 tool_use_id 不参与去重
        let new = req(11, "s1", None);
        assert!(!replace_duplicate_in_queue(&mut q, new));
    }

    #[test]
    fn questions_advance_and_build_typed_answers() {
        let option = |label: &str| PromptOption {
            label: label.into(),
            description: None,
        };
        let mut prompt = PendingPrompt::Questions {
            questions: vec![
                ChoiceQuestion {
                    key: "Mode?".into(),
                    prompt: "Mode?".into(),
                    options: vec![option("Fast"), option("Safe")],
                    multi_select: false,
                    selected: Vec::new(),
                },
                ChoiceQuestion {
                    key: "Checks?".into(),
                    prompt: "Checks?".into(),
                    options: vec![option("Tests"), option("Lint")],
                    multi_select: true,
                    selected: Vec::new(),
                },
            ],
            current: 0,
        };

        assert_eq!(prompt.select_option(1), Some(QuestionProgress::Updated));
        assert_eq!(
            prompt.current_question().map(|(_, index, _)| index),
            Some(1)
        );
        assert_eq!(prompt.select_option(0), Some(QuestionProgress::Updated));
        assert_eq!(prompt.select_option(1), Some(QuestionProgress::Updated));
        let Some(QuestionProgress::Complete(answers)) = prompt.submit_current() else {
            panic!("expected completed answers");
        };
        assert_eq!(answers["Mode?"], serde_json::json!("Safe"));
        assert_eq!(answers["Checks?"], serde_json::json!(["Tests", "Lint"]));
    }

    #[test]
    fn multi_select_requires_a_selection_before_submit() {
        let mut prompt = PendingPrompt::Questions {
            questions: vec![ChoiceQuestion {
                key: "Checks?".into(),
                prompt: "Checks?".into(),
                options: vec![PromptOption {
                    label: "Tests".into(),
                    description: None,
                }],
                multi_select: true,
                selected: Vec::new(),
            }],
            current: 0,
        };
        assert_eq!(prompt.submit_current(), None);
        assert_eq!(prompt.select_option(99), None);
    }
}
