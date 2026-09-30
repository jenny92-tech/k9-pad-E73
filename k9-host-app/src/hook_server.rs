// INPUT:  tokio (UnixListener/oneshot), serde_json, session (HookEvent, 活动/完成事件谓词), app_state (AppEvent), bridge (DialogRequest), k9_datachannel_proto (DialogResultCode), std::sync::mpsc
// OUTPUT: spawn_hook_server()、PendingMap、审批/Questions 解析与响应、设备弹窗映射、pending 生命周期 drain
// POS:    AI 审批面板入口 — 跑在 bridge 的 tokio 线程，接收 k9-hook-bridge / Pi 扩展的 hook JSON：
//         普通事件转发 GPUI 事件通道，阻塞事件注册 oneshot waiter 挂起直到 UI/设备决策（同时推 show_dialog 到设备）；
//         Codex auto_review/guardian_subagent 的 PermissionRequest 让路给 Codex 自身 reviewer（直接回 {}）；
//         Phase 4 健壮性：SessionEnd → deny 该 session 全部 pending；「已在终端批准」启发式（活动事件 allow 无 tool_use_id 的 pending）；
//         同 tool_use_id 完成事件 deny 释放；同 tool_use_id 重复请求 deny 旧的换新的

use std::collections::HashMap;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{mpsc, Arc, Mutex};

use k9_datachannel_proto::{DialogKind, DialogResultCode};
use log::{error, info, warn};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::UnixListener;
use tokio::sync::oneshot;

use crate::app_state::{AppEvent, ChoiceQuestion, PendingPrompt, PromptOption};
use crate::bridge::DialogRequest;
use crate::session::{self, HookEvent};

/// 单个连接最大读取字节（蓝本上限 10MB）
const MAX_EVENT_BYTES: u64 = 10 * 1024 * 1024;

/// show_dialog 文本上限（k9-host-lib：MAX_PAYLOAD_SIZE - 2 = 58 字节）
const MAX_DIALOG_TEXT: usize = 58;

/// 审批决策 waiter：决策 JSON 回写通道 + 归属信息（Phase 4 drain/去重索引用，
/// 单 map 最小改动，不另建 req→session 索引）
pub struct PendingEntry {
    pub session_id: String,
    pub tool_use_id: Option<String>,
    pub prompt: PendingPrompt,
    pub updated_input: Option<serde_json::Value>,
    pub tx: oneshot::Sender<serde_json::Value>,
}

/// 审批决策 waiter 表：req_id → waiter（先答先生效，remove 即消费）
pub type PendingMap = Arc<Mutex<HashMap<u64, PendingEntry>>>;

/// 设备弹窗 → 权限请求映射：dialog_id → req_id（DialogResult 命中即消费；
/// 下发失败/设备不支持时条目残留无害——keyspace 仅 255，且命中一次即 remove）
pub type DialogMap = Arc<Mutex<HashMap<u8, u64>>>;

/// socket 路径：`K9PAD_SOCKET_PATH` 覆盖，默认 `/tmp/k9pad-{uid}.sock`
pub fn socket_path() -> PathBuf {
    if let Ok(p) = std::env::var("K9PAD_SOCKET_PATH") {
        if !p.is_empty() {
            return PathBuf::from(p);
        }
    }
    PathBuf::from(format!("/tmp/k9pad-{}.sock", current_uid()))
}

fn current_uid() -> String {
    std::process::Command::new("id")
        .arg("-u")
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "0".into())
}

/// req_id → dialog_id（1..=255，避开 0）
pub fn req_to_dialog_id(req_id: u64) -> u8 {
    (req_id % 255 + 1) as u8
}

/// 设备弹窗文本：`<source>: <tool> <summary>`（tool 为空时省略），
/// ASCII 安全（非 ASCII 字符替换为 '?'，固件无 CJK 字体），≤58 字节截断
pub fn dialog_text(source: &str, tool_name: &str, summary: &str) -> String {
    let raw = if tool_name.is_empty() {
        format!("{source}: {summary}")
    } else {
        format!("{source}: {tool_name} {summary}")
    };
    raw.chars()
        .map(|c| if c.is_ascii() { c } else { '?' })
        .take(MAX_DIALOG_TEXT)
        .collect()
}

/// DialogResult → 审批决策：Confirm→allow，Cancel/Timeout→deny
pub fn dialog_decision(selection: u8, result: DialogResultCode) -> &'static str {
    // ConfirmCancel 弹窗选项：[0]=Allow [1]=Deny；Cancel/Timeout 一律 deny
    match result {
        DialogResultCode::Confirm => {
            if selection == 0 {
                "allow"
            } else {
                "deny"
            }
        }
        _ => "deny",
    }
}

/// 审批决策响应 JSON（格式严格照蓝本，Claude/Codex 同构，Pi 只查 decision.behavior）
pub fn decision_json(decision: &str) -> serde_json::Value {
    decision_json_with_updated_input(decision, None)
}

fn decision_json_with_updated_input(
    decision: &str,
    updated_input: Option<serde_json::Value>,
) -> serde_json::Value {
    let mut decision_value = serde_json::json!({ "behavior": decision });
    if let Some(updated_input) = updated_input {
        decision_value["updatedInput"] = updated_input;
    }
    serde_json::json!({
        "hookSpecificOutput": {
            "hookEventName": "PermissionRequest",
            "decision": decision_value
        }
    })
}

fn question_decision_json(
    mut updated_input: Option<serde_json::Value>,
    answers: serde_json::Map<String, serde_json::Value>,
) -> serde_json::Value {
    let input = updated_input.get_or_insert_with(|| serde_json::json!({}));
    if !input.is_object() {
        *input = serde_json::json!({});
    }
    input["answers"] = serde_json::Value::Object(answers);
    decision_json_with_updated_input("allow", updated_input)
}

/// 完成指定 req 的 waiter（先答先生效：remove 即消费，后到者无 waiter 忽略）。
/// 有 waiter 被完成返回 true。
pub fn resolve_pending(pending: &PendingMap, req_id: u64, decision: &str) -> bool {
    let entry = pending.lock().unwrap().remove(&req_id);
    match entry {
        Some(e) => {
            info!("Permission #{req_id} resolved: {decision}");
            let _ = e.tx.send(decision_json(decision));
            true
        }
        None => false,
    }
}

pub fn resolve_questions(
    pending: &PendingMap,
    req_id: u64,
    answers: serde_json::Map<String, serde_json::Value>,
) -> bool {
    let entry = pending.lock().unwrap().remove(&req_id);
    match entry {
        Some(entry) => {
            info!(
                "Question #{req_id} resolved with {} answer(s)",
                answers.len()
            );
            let response = question_decision_json(entry.updated_input, answers);
            let _ = entry.tx.send(response);
            true
        }
        None => false,
    }
}

/// 设备 DialogResult 完成普通审批或单道单选题；复杂 Questions 只在桌面面板处理。
pub fn resolve_dialog_selection(
    pending: &PendingMap,
    req_id: u64,
    selection: u8,
    result: DialogResultCode,
) -> Option<String> {
    let entry = pending.lock().unwrap().remove(&req_id)?;
    let PendingEntry {
        prompt,
        updated_input,
        tx,
        ..
    } = entry;
    let (response, description) = match (&prompt, result) {
        (PendingPrompt::Questions { questions, .. }, DialogResultCode::Confirm)
            if questions.len() == 1 && !questions[0].multi_select =>
        {
            let question = &questions[0];
            let Some(option) = question.options.get(selection as usize) else {
                let response = decision_json("deny");
                let _ = tx.send(response);
                return Some("deny (invalid selection)".into());
            };
            let mut answers = serde_json::Map::new();
            answers.insert(
                question.key.clone(),
                serde_json::Value::String(option.label.clone()),
            );
            (
                question_decision_json(updated_input, answers),
                format!("option {}", option.label),
            )
        }
        (PendingPrompt::Questions { .. }, _) => (decision_json("deny"), "deny".to_string()),
        (PendingPrompt::Approval, _) => {
            let decision = dialog_decision(selection, result);
            (decision_json(decision), decision.to_string())
        }
    };
    info!("Request #{req_id} resolved from device: {description}");
    let _ = tx.send(response);
    Some(description)
}

/// SessionEnd / 会话被移除（照蓝本 removeSession → drainPermissions）：
/// deny 该 session 全部 pending，返回被 deny 的 req_ids。
pub fn deny_session_pending(pending: &PendingMap, session_id: &str, reason: &str) -> Vec<u64> {
    let entries: Vec<(u64, PendingEntry)> = {
        let mut map = pending.lock().unwrap();
        let ids: Vec<u64> = map
            .iter()
            .filter(|(_, e)| e.session_id == session_id)
            .map(|(id, _)| *id)
            .collect();
        ids.into_iter()
            .filter_map(|id| map.remove(&id).map(|e| (id, e)))
            .collect()
    };
    let mut ids = Vec::with_capacity(entries.len());
    for (req_id, e) in entries {
        info!("Permission #{req_id} denied by {reason} (session {session_id})");
        let _ = e.tx.send(decision_json("deny"));
        ids.push(req_id);
    }
    ids
}

/// 「已在终端批准」启发式 + 同 tool_use_id 完成释放（照蓝本
/// resolveOrphanPermissionsOnActivity / resolveToolUseIfCompleted）：
/// - 活动事件（PreToolUse/PostToolUse/PostToolUseFailure/Stop/UserPromptSubmit）→
///   该 session 无 tool_use_id 的 pending 视为已在终端批准，**allow** 释放；
/// - 完成事件（PostToolUse/PostToolUseFailure/PermissionDenied）携带 tool_use_id →
///   同 id 的 pending 已被 agent 跳过/完成，**deny** 释放。
///
/// 有 tool_use_id 的请求不被活动事件误伤（蓝本 #147：并行工具调用不能互相 deny）。
/// 返回 (allowed_ids, denied_ids)。
pub fn drain_pending_on_activity(pending: &PendingMap, ev: &HookEvent) -> (Vec<u64>, Vec<u64>) {
    let is_activity = session::is_activity_event(&ev.event_name);
    let is_completion = session::is_tool_completion_event(&ev.event_name);
    if !is_activity && !is_completion {
        return (Vec::new(), Vec::new());
    }
    let (allowed, denied) = {
        let mut map = pending.lock().unwrap();
        let allow_ids: Vec<u64> = if is_activity {
            map.iter()
                .filter(|(_, e)| e.session_id == ev.session_id && e.tool_use_id.is_none())
                .map(|(id, _)| *id)
                .collect()
        } else {
            Vec::new()
        };
        let deny_ids: Vec<u64> = match (&ev.tool_use_id, is_completion) {
            (Some(tuid), true) => map
                .iter()
                .filter(|(_, e)| e.tool_use_id.as_deref() == Some(tuid.as_str()))
                .map(|(id, _)| *id)
                .collect(),
            _ => Vec::new(),
        };
        let allowed: Vec<(u64, PendingEntry)> = allow_ids
            .into_iter()
            .filter_map(|id| map.remove(&id).map(|e| (id, e)))
            .collect();
        let denied: Vec<(u64, PendingEntry)> = deny_ids
            .into_iter()
            .filter_map(|id| map.remove(&id).map(|e| (id, e)))
            .collect();
        (allowed, denied)
    };
    let mut allow_ids = Vec::with_capacity(allowed.len());
    for (req_id, e) in allowed {
        info!(
            "Permission #{req_id} auto-allowed (answered in terminal, session {}, event {})",
            ev.session_id, ev.event_name
        );
        let _ = e.tx.send(decision_json("allow"));
        allow_ids.push(req_id);
    }
    let mut deny_ids = Vec::with_capacity(denied.len());
    for (req_id, e) in denied {
        info!(
            "Permission #{req_id} denied (tool_use_id completed, event {})",
            ev.event_name
        );
        let _ = e.tx.send(decision_json("deny"));
        deny_ids.push(req_id);
    }
    (allow_ids, deny_ids)
}

/// 同 tool_use_id 重复 PermissionRequest（照蓝本 mergeDuplicatePermissionRequest）：
/// deny 旧 waiter 并移除，返回旧 req_id；无重复返回 None。
/// 与蓝本差异：蓝本 #169 还要求 tool_input 一致才算重放（防并行调用同 id 误伤），
/// v1 简化——只要同 tool_use_id 即视为重放。
pub fn deny_duplicate_tool_use(pending: &PendingMap, tool_use_id: &str) -> Option<u64> {
    let old = {
        let mut map = pending.lock().unwrap();
        let old_id = map
            .iter()
            .find(|(_, e)| e.tool_use_id.as_deref() == Some(tool_use_id))
            .map(|(id, _)| *id);
        old_id.and_then(|id| map.remove(&id).map(|e| (id, e)))
    };
    old.map(|(req_id, e)| {
        info!("Permission #{req_id} denied (duplicate tool_use_id {tool_use_id} replay)");
        let _ = e.tx.send(decision_json("deny"));
        req_id
    })
}

/// 阻塞事件判定：PermissionRequest，或携带 question 的 Notification
fn is_blocking(v: &serde_json::Value, event: &HookEvent) -> bool {
    event.event_name == "PermissionRequest"
        || (event.event_name == "Notification" && event.has_question && {
            // question 判定以原始 payload 为准（与 HookEvent::from_json 一致）
            v.get("question").and_then(|q| q.as_str()).is_some()
        })
}

/// Codex 特例（照蓝本 CodexPermissionRules.shouldDeferToCodexAutoReview）：
/// payload 带 `approvals_reviewer`（含 approvalsReviewer/_approvals_reviewer 别名）
/// 且值为 auto_review/guardian_subagent 时，审批让给 Codex 自己的 reviewer，
/// 不入审批队列直接回 `{}`；事件本身也不进 reducer（避免会话卡在 waitingApproval）。
/// 第一个非空 reviewer 值即定论。
fn defer_to_codex_reviewer(v: &serde_json::Value, event: &HookEvent) -> bool {
    if event.event_name != "PermissionRequest" || event.source != "codex" {
        return false;
    }
    for key in [
        "approvals_reviewer",
        "approvalsReviewer",
        "_approvals_reviewer",
    ] {
        if let Some(raw) = v.get(key).and_then(|x| x.as_str()) {
            let normalized = raw
                .trim()
                .trim_matches(|c| c == '"' || c == '\'')
                .to_lowercase();
            if normalized.is_empty() {
                continue;
            }
            return normalized == "auto_review" || normalized == "guardian_subagent";
        }
    }
    false
}

/// 审批摘要（卡片显示用，预生成字符串）：
/// Bash→command 前 40 字符；Edit/Write→file_path；Read→file_path+offset/limit；
/// Grep/Glob→pattern；其他→tool_input 第一个字符串 value 前 40 字符
fn summarize(v: &serde_json::Value) -> String {
    let tool = v.get("tool_name").and_then(|x| x.as_str()).unwrap_or("");
    let input = v
        .get("tool_input")
        .cloned()
        .unwrap_or(serde_json::Value::Null);
    let get_str = |key: &str| input.get(key).and_then(|x| x.as_str());
    let truncate = |s: &str| -> String { s.chars().take(40).collect() };

    match tool {
        "Bash" => get_str("command").map(truncate).unwrap_or_default(),
        "Edit" | "Write" => get_str("file_path").unwrap_or("").to_string(),
        "Read" => {
            let path = get_str("file_path").unwrap_or("");
            let offset = input.get("offset").and_then(|x| x.as_u64());
            let limit = input.get("limit").and_then(|x| x.as_u64());
            match (offset, limit) {
                (Some(o), Some(l)) => format!("{path} offset={o} limit={l}"),
                (Some(o), None) => format!("{path} offset={o}"),
                (None, Some(l)) => format!("{path} limit={l}"),
                (None, None) => path.to_string(),
            }
        }
        "Grep" | "Glob" => get_str("pattern").map(truncate).unwrap_or_default(),
        _ => {
            // Notification 提问：用 question 文本；否则取 tool_input 第一个字符串 value
            if let Some(q) = v.get("question").and_then(|x| x.as_str()) {
                return truncate(q);
            }
            input
                .as_object()
                .and_then(|obj| obj.values().find_map(|x| x.as_str()))
                .map(truncate)
                .unwrap_or_default()
        }
    }
}

fn request_prompt(
    value: &serde_json::Value,
    event: &HookEvent,
) -> (PendingPrompt, Option<serde_json::Value>) {
    if event.tool_name.as_deref() != Some("AskUserQuestion") {
        return (PendingPrompt::Approval, None);
    }
    let Some(input) = value.get("tool_input") else {
        return (PendingPrompt::Approval, None);
    };
    let Some(raw_questions) = input
        .get("questions")
        .and_then(|questions| questions.as_array())
    else {
        return (PendingPrompt::Approval, None);
    };

    let mut key_counts = HashMap::<String, usize>::new();
    let mut questions = Vec::new();
    for raw in raw_questions {
        let prompt = raw
            .get("question")
            .and_then(|question| question.as_str())
            .filter(|question| !question.trim().is_empty())
            .unwrap_or("Question")
            .to_string();
        let options: Vec<PromptOption> = raw
            .get("options")
            .and_then(|options| options.as_array())
            .into_iter()
            .flatten()
            .filter_map(|option| {
                let label = option.get("label").and_then(|label| label.as_str())?.trim();
                if label.is_empty() {
                    return None;
                }
                Some(PromptOption {
                    label: label.to_string(),
                    description: option
                        .get("description")
                        .and_then(|description| description.as_str())
                        .filter(|description| !description.trim().is_empty())
                        .map(str::to_string),
                })
            })
            .collect();
        if options.is_empty() {
            continue;
        }
        let count = key_counts.entry(prompt.clone()).or_default();
        *count += 1;
        let key = if *count == 1 {
            prompt.clone()
        } else {
            format!("{prompt}_{}", *count)
        };
        questions.push(ChoiceQuestion {
            key,
            prompt,
            options,
            multi_select: raw
                .get("multiSelect")
                .and_then(|multi| multi.as_bool())
                .unwrap_or(false),
            selected: Vec::new(),
        });
    }

    if questions.is_empty() {
        (PendingPrompt::Approval, None)
    } else {
        (
            PendingPrompt::Questions {
                questions,
                current: 0,
            },
            Some(input.clone()),
        )
    }
}

/// 在 tokio runtime 内 spawn hook socket server。
///
/// 返回共享 pending map（供命令通道完成决策）与 dialog→req 映射（供设备
/// DialogResult 反查 req_id）；`dialog_tx` 为设备弹窗下发通道（dispatcher 队列）。
pub fn spawn_hook_server(
    event_tx: mpsc::Sender<AppEvent>,
    dialog_tx: tokio::sync::mpsc::UnboundedSender<DialogRequest>,
) -> (PendingMap, DialogMap) {
    let pending: PendingMap = Arc::new(Mutex::new(HashMap::new()));
    let dialogs: DialogMap = Arc::new(Mutex::new(HashMap::new()));
    let pending_clone = pending.clone();
    let dialogs_clone = dialogs.clone();
    tokio::spawn(async move {
        if let Err(e) = run(event_tx, dialog_tx, pending_clone, dialogs_clone).await {
            error!("hook server exited: {e}");
        }
    });
    (pending, dialogs)
}

async fn run(
    event_tx: mpsc::Sender<AppEvent>,
    dialog_tx: tokio::sync::mpsc::UnboundedSender<DialogRequest>,
    pending: PendingMap,
    dialogs: DialogMap,
) -> std::io::Result<()> {
    let path = socket_path();
    // bind 前清理旧 socket 文件
    let _ = std::fs::remove_file(&path);
    let listener = UnixListener::bind(&path)?;
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o700))?;
    info!("hook server listening on {}", path.display());

    let next_req_id = Arc::new(AtomicU64::new(1));
    loop {
        let (stream, _) = listener.accept().await?;
        let event_tx = event_tx.clone();
        let dialog_tx = dialog_tx.clone();
        let pending = pending.clone();
        let dialogs = dialogs.clone();
        let next_req_id = next_req_id.clone();
        tokio::spawn(async move {
            if let Err(e) =
                handle_conn(stream, event_tx, dialog_tx, pending, dialogs, next_req_id).await
            {
                warn!("hook connection error: {e}");
            }
        });
    }
}

async fn handle_conn(
    mut stream: tokio::net::UnixStream,
    event_tx: mpsc::Sender<AppEvent>,
    dialog_tx: tokio::sync::mpsc::UnboundedSender<DialogRequest>,
    pending: PendingMap,
    dialogs: DialogMap,
    next_req_id: Arc<AtomicU64>,
) -> std::io::Result<()> {
    // 分帧照蓝本：client 半关闭表结束，server 读到 EOF（10MB 上限）
    let mut buf = Vec::new();
    (&mut stream)
        .take(MAX_EVENT_BYTES)
        .read_to_end(&mut buf)
        .await?;

    let value: serde_json::Value = match serde_json::from_slice(&buf) {
        Ok(v) => v,
        Err(e) => {
            warn!("hook event parse failed: {e}");
            stream.write_all(br#"{"error":"parse_failed"}"#).await?;
            stream.shutdown().await?;
            return Ok(());
        }
    };

    let event = HookEvent::from_json(&value);

    // Codex auto_review/guardian_subagent：让路给 Codex 自身 reviewer，
    // 不入审批队列、不进 reducer，直接回 {}（照蓝本 shouldDeferPermissionRequestToProvider）
    if defer_to_codex_reviewer(&value, &event) {
        stream.write_all(b"{}").await?;
        stream.shutdown().await?;
        return Ok(());
    }

    if is_blocking(&value, &event) {
        let req_id = next_req_id.fetch_add(1, Ordering::Relaxed);
        let (tx, rx) = oneshot::channel::<serde_json::Value>();
        let tool_use_id = event.tool_use_id.clone();
        let (prompt, updated_input) = request_prompt(&value, &event);

        // 同 tool_use_id 重复请求（蓝本 mergeDuplicatePermissionRequest）：
        // deny 旧 waiter，新的替换入队（GPUI 侧队列在 AppEvent::PermissionRequest 处理处原位替换）
        if let Some(tuid) = &tool_use_id {
            deny_duplicate_tool_use(&pending, tuid);
        }
        pending.lock().unwrap().insert(
            req_id,
            PendingEntry {
                session_id: event.session_id.clone(),
                tool_use_id: tool_use_id.clone(),
                prompt: prompt.clone(),
                updated_input,
                tx,
            },
        );

        let summary = prompt
            .current_question()
            .map(|(question, _, _)| question.prompt.clone())
            .unwrap_or_else(|| summarize(&value));

        // 设备应答渠道：登记 dialog→req 映射并推 show_dialog 到 dispatcher 队列。
        // 设备支持普通审批及单道单选题；多题/多选由桌面面板完成。
        let dialog_id = req_to_dialog_id(req_id);
        let device_dialog = match &prompt {
            PendingPrompt::Approval => Some(DialogRequest {
                id: Some(dialog_id),
                kind: DialogKind::ConfirmCancel,
                title: dialog_text(
                    &event.source,
                    event.tool_name.as_deref().unwrap_or(""),
                    &summary,
                ),
                options: vec!["Allow".to_string(), "Deny".to_string()],
            }),
            PendingPrompt::Questions { questions, .. }
                if questions.len() == 1 && !questions[0].multi_select =>
            {
                Some(DialogRequest {
                    id: Some(dialog_id),
                    kind: DialogKind::Choice,
                    title: dialog_text(&event.source, "", &questions[0].prompt),
                    options: questions[0]
                        .options
                        .iter()
                        .map(|option| option.label.clone())
                        .collect(),
                })
            }
            PendingPrompt::Questions { .. } => None,
        };
        if let Some(dialog) = device_dialog {
            dialogs.lock().unwrap().insert(dialog_id, req_id);
            match dialog_tx.send(dialog) {
                Ok(()) => info!("Request #{req_id} queued to device as dialog #{dialog_id}"),
                Err(_) => warn!("dialog channel closed, request #{req_id} device channel disabled"),
            }
        } else {
            info!("Request #{req_id} requires desktop multi-question/multi-select UI");
        }

        let _ = event_tx.send(AppEvent::PermissionRequest {
            req_id,
            session_id: event.session_id.clone(),
            source: event.source.clone(),
            tool_name: event.tool_name.clone().unwrap_or_default(),
            summary,
            prompt,
            tool_use_id,
        });

        // 无超时等 UI/设备决策（蓝本：App 侧权限请求不设超时）
        //
        // 与蓝本差异（peer 断连检测）：蓝本 handlePeerDisconnect 在 bridge socket 断开时
        // deny 该 session 全部 pending。本协议 client 发完即 shutdown(Write) 半关闭，
        // tokio UnixStream 上半关闭后的 read 立即返回 Ok(0)，与对端进程死亡（全关闭）
        // 无法区分；write 探测又会污染响应协议。故 v1 不做主动断连检测，对端死亡依赖：
        // SessionEnd deny drain + 「已在终端批准」启发式 + 看门狗 sweep 兜底。
        if let Ok(decision) = rx.await {
            let body = serde_json::to_vec(&decision).unwrap_or_else(|_| b"{}".to_vec());
            stream.write_all(&body).await?;
        }
        // oneshot sender 被丢弃（pending 被清理）→ 回 `{}` 让 CLI 回退自身审批
        else {
            stream.write_all(b"{}").await?;
        }
        stream.shutdown().await?;
    } else {
        // Phase 4 健壮性 drain（先答先生效，GPUI 侧队列镜像同步出队）：
        // - SessionEnd：会话结束，deny 其全部 pending（蓝本 removeSession 语义）；
        // - 活动/完成事件：「已在终端批准」启发式 allow + 同 tool_use_id 完成 deny。
        if event.event_name == "SessionEnd" {
            deny_session_pending(&pending, &event.session_id, "session-end");
        } else {
            drain_pending_on_activity(&pending, &event);
        }
        let _ = event_tx.send(AppEvent::HookEvent(event));
        stream.write_all(b"{}").await?;
        stream.shutdown().await?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decision_json_matches_blueprint() {
        let v = decision_json("allow");
        assert_eq!(
            v,
            serde_json::json!({
                "hookSpecificOutput": {
                    "hookEventName": "PermissionRequest",
                    "decision": {"behavior": "allow"}
                }
            })
        );
        assert_eq!(
            decision_json("deny")["hookSpecificOutput"]["decision"]["behavior"],
            "deny"
        );
    }

    #[test]
    fn ask_user_question_parses_options_and_deduplicates_answer_keys() {
        let value = serde_json::json!({
            "hook_event_name": "PermissionRequest",
            "tool_name": "AskUserQuestion",
            "tool_input": {
                "questions": [
                    {
                        "question": "Mode?",
                        "options": [
                            {"label": "Fast", "description": "Less validation"},
                            {"label": "Safe"}
                        ]
                    },
                    {
                        "question": "Mode?",
                        "multiSelect": true,
                        "options": [{"label": "Tests"}, {"label": "Lint"}]
                    }
                ]
            }
        });
        let event = HookEvent::from_json(&value);
        let (prompt, updated_input) = request_prompt(&value, &event);
        let PendingPrompt::Questions { questions, current } = prompt else {
            panic!("expected questions");
        };
        assert_eq!(current, 0);
        assert_eq!(questions[0].key, "Mode?");
        assert_eq!(questions[1].key, "Mode?_2");
        assert_eq!(
            questions[0].options[0].description.as_deref(),
            Some("Less validation")
        );
        assert!(questions[1].multi_select);
        assert_eq!(
            updated_input.unwrap()["questions"]
                .as_array()
                .unwrap()
                .len(),
            2
        );
    }

    #[test]
    fn question_response_preserves_input_and_adds_answers() {
        let pending = new_pending();
        let (tx, rx) = oneshot::channel();
        pending.lock().unwrap().insert(
            7,
            PendingEntry {
                session_id: "s1".into(),
                tool_use_id: None,
                prompt: PendingPrompt::Questions {
                    questions: Vec::new(),
                    current: 0,
                },
                updated_input: Some(serde_json::json!({"questions": ["original"]})),
                tx,
            },
        );
        let mut answers = serde_json::Map::new();
        answers.insert("Mode?".into(), serde_json::json!("Safe"));
        assert!(resolve_questions(&pending, 7, answers));
        let response = rx.blocking_recv().unwrap();
        let input = &response["hookSpecificOutput"]["decision"]["updatedInput"];
        assert_eq!(input["questions"], serde_json::json!(["original"]));
        assert_eq!(input["answers"]["Mode?"], "Safe");
    }

    #[test]
    fn summarize_rules() {
        // Bash → command 前 40 字符
        let v =
            serde_json::json!({"tool_name": "Bash", "tool_input": {"command": "rm -rf /tmp/x"}});
        assert_eq!(summarize(&v), "rm -rf /tmp/x");
        let long = "a".repeat(100);
        let v = serde_json::json!({"tool_name": "Bash", "tool_input": {"command": long}});
        assert_eq!(summarize(&v).chars().count(), 40);

        // Edit/Write → file_path
        let v = serde_json::json!({"tool_name": "Edit", "tool_input": {"file_path": "/tmp/a.rs"}});
        assert_eq!(summarize(&v), "/tmp/a.rs");
        let v = serde_json::json!({"tool_name": "Write", "tool_input": {"file_path": "/tmp/b.rs"}});
        assert_eq!(summarize(&v), "/tmp/b.rs");

        // Read → file_path + offset/limit
        let v = serde_json::json!({"tool_name": "Read", "tool_input": {"file_path": "/tmp/c.rs", "offset": 10, "limit": 50}});
        assert_eq!(summarize(&v), "/tmp/c.rs offset=10 limit=50");
        let v = serde_json::json!({"tool_name": "Read", "tool_input": {"file_path": "/tmp/c.rs"}});
        assert_eq!(summarize(&v), "/tmp/c.rs");

        // Grep/Glob → pattern
        let v = serde_json::json!({"tool_name": "Grep", "tool_input": {"pattern": "fn main"}});
        assert_eq!(summarize(&v), "fn main");
        let v = serde_json::json!({"tool_name": "Glob", "tool_input": {"pattern": "**/*.rs"}});
        assert_eq!(summarize(&v), "**/*.rs");

        // 其他 → 第一个字符串 value 前 40 字符
        let v = serde_json::json!({"tool_name": "WebFetch", "tool_input": {"url": "https://example.com"}});
        assert_eq!(summarize(&v), "https://example.com");

        // question 兜底
        let v = serde_json::json!({"hook_event_name": "Notification", "question": "继续吗？"});
        assert_eq!(summarize(&v), "继续吗？");
    }

    #[test]
    fn blocking_detection() {
        let v = serde_json::json!({"hook_event_name": "PermissionRequest", "tool_name": "Bash"});
        let e = HookEvent::from_json(&v);
        assert!(is_blocking(&v, &e));

        let v = serde_json::json!({"hook_event_name": "Notification", "question": "?"});
        let e = HookEvent::from_json(&v);
        assert!(is_blocking(&v, &e));

        let v = serde_json::json!({"hook_event_name": "Notification", "message": "hi"});
        let e = HookEvent::from_json(&v);
        assert!(!is_blocking(&v, &e));

        let v = serde_json::json!({"hook_event_name": "PreToolUse", "tool_name": "Bash"});
        let e = HookEvent::from_json(&v);
        assert!(!is_blocking(&v, &e));
    }

    #[test]
    fn codex_auto_review_deferral() {
        // auto_review / guardian_subagent → 让路
        for reviewer in [
            "auto_review",
            "guardian_subagent",
            "AUTO_REVIEW",
            " \"auto_review\" ",
        ] {
            let v = serde_json::json!({
                "hook_event_name": "PermissionRequest",
                "_source": "codex",
                "approvals_reviewer": reviewer
            });
            let e = HookEvent::from_json(&v);
            assert!(defer_to_codex_reviewer(&v, &e), "reviewer: {reviewer}");
        }

        // 别名 key 同样生效
        for key in ["approvalsReviewer", "_approvals_reviewer"] {
            let v = serde_json::json!({
                "hook_event_name": "PermissionRequest",
                "_source": "codex",
                key: "guardian_subagent"
            });
            let e = HookEvent::from_json(&v);
            assert!(defer_to_codex_reviewer(&v, &e), "key: {key}");
        }

        // 其他 reviewer 值 → 正常入队
        let v = serde_json::json!({
            "hook_event_name": "PermissionRequest",
            "_source": "codex",
            "approvals_reviewer": "user"
        });
        let e = HookEvent::from_json(&v);
        assert!(!defer_to_codex_reviewer(&v, &e));

        // 无 reviewer 字段 → 正常入队
        let v = serde_json::json!({"hook_event_name": "PermissionRequest", "_source": "codex"});
        let e = HookEvent::from_json(&v);
        assert!(!defer_to_codex_reviewer(&v, &e));

        // 非 codex 来源不触发
        let v = serde_json::json!({
            "hook_event_name": "PermissionRequest",
            "_source": "claude",
            "approvals_reviewer": "auto_review"
        });
        let e = HookEvent::from_json(&v);
        assert!(!defer_to_codex_reviewer(&v, &e));

        // 非 PermissionRequest 不触发
        let v = serde_json::json!({
            "hook_event_name": "Notification",
            "_source": "codex",
            "approvals_reviewer": "auto_review"
        });
        let e = HookEvent::from_json(&v);
        assert!(!defer_to_codex_reviewer(&v, &e));
    }

    #[test]
    fn req_dialog_id_mapping() {
        assert_eq!(req_to_dialog_id(0), 1);
        assert_eq!(req_to_dialog_id(1), 2);
        assert_eq!(req_to_dialog_id(254), 255);
        // 回绕：255 → 1，256 → 2（永不产生 0）
        assert_eq!(req_to_dialog_id(255), 1);
        assert_eq!(req_to_dialog_id(256), 2);
        assert_eq!(req_to_dialog_id(u64::MAX), (u64::MAX % 255 + 1) as u8);
    }

    #[test]
    fn dialog_text_ascii_safe_and_truncated() {
        // 常规格式：source: tool summary
        assert_eq!(
            dialog_text("claude", "Bash", "ls -la"),
            "claude: Bash ls -la"
        );
        // tool 为空时省略
        assert_eq!(dialog_text("codex", "", "proceed?"), "codex: proceed?");
        // 非 ASCII → '?'
        assert_eq!(dialog_text("claude", "", "继续吗？"), "claude: ????");
        // 超长按 58 字节截断（过滤后全 ASCII，chars == bytes）
        let long = dialog_text("claude", "Bash", &"a".repeat(100));
        assert_eq!(long.len(), 58);
        assert!(long.is_ascii());
    }

    #[test]
    fn dialog_result_to_decision() {
        assert_eq!(dialog_decision(0, DialogResultCode::Confirm), "allow");
        assert_eq!(dialog_decision(0, DialogResultCode::Cancel), "deny");
        assert_eq!(dialog_decision(0, DialogResultCode::Timeout), "deny");
    }

    // ---- Phase 4：pending 生命周期 drain ----

    fn new_pending() -> PendingMap {
        Arc::new(Mutex::new(HashMap::new()))
    }

    fn insert_req(
        pending: &PendingMap,
        req_id: u64,
        session_id: &str,
        tool_use_id: Option<&str>,
    ) -> oneshot::Receiver<serde_json::Value> {
        let (tx, rx) = oneshot::channel();
        pending.lock().unwrap().insert(
            req_id,
            PendingEntry {
                session_id: session_id.into(),
                tool_use_id: tool_use_id.map(|s| s.into()),
                prompt: PendingPrompt::Approval,
                updated_input: None,
                tx,
            },
        );
        rx
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

    fn decision_of(v: &serde_json::Value) -> &str {
        v["hookSpecificOutput"]["decision"]["behavior"]
            .as_str()
            .unwrap()
    }

    #[test]
    fn session_end_denies_all_pending_of_session() {
        let pending = new_pending();
        let rx1 = insert_req(&pending, 1, "s1", None);
        let rx2 = insert_req(&pending, 2, "s1", Some("toolu_a"));
        let rx3 = insert_req(&pending, 3, "s2", None);

        let mut denied = deny_session_pending(&pending, "s1", "session-end");
        denied.sort_unstable();
        assert_eq!(denied, vec![1, 2]);
        assert_eq!(decision_of(&rx1.blocking_recv().unwrap()), "deny");
        assert_eq!(decision_of(&rx2.blocking_recv().unwrap()), "deny");
        // s2 不受影响
        assert!(pending.lock().unwrap().contains_key(&3));
        drop(rx3);
    }

    #[test]
    fn activity_allows_orphans_only() {
        let pending = new_pending();
        let rx_orphan = insert_req(&pending, 1, "s1", None);
        let rx_tracked = insert_req(&pending, 2, "s1", Some("toolu_a"));
        let rx_other = insert_req(&pending, 3, "s2", None);

        // 活动事件：allow 该 session 无 tool_use_id 的 pending；有 id 的和其他 session 不动
        let (allowed, denied) = drain_pending_on_activity(&pending, &ev("Stop", "s1", None));
        assert_eq!(allowed, vec![1]);
        assert!(denied.is_empty());
        assert_eq!(decision_of(&rx_orphan.blocking_recv().unwrap()), "allow");
        assert!(pending.lock().unwrap().contains_key(&2));
        assert!(pending.lock().unwrap().contains_key(&3));
        drop((rx_tracked, rx_other));

        // 非活动事件不触发
        let pending = new_pending();
        let _rx = insert_req(&pending, 1, "s1", None);
        let (allowed, denied) =
            drain_pending_on_activity(&pending, &ev("SessionStart", "s1", None));
        assert!(allowed.is_empty() && denied.is_empty());
        assert!(pending.lock().unwrap().contains_key(&1));
    }

    #[test]
    fn completion_event_denies_same_tool_use_id() {
        let pending = new_pending();
        let rx1 = insert_req(&pending, 1, "s1", Some("toolu_a"));
        let rx2 = insert_req(&pending, 2, "s1", Some("toolu_b"));

        for name in ["PostToolUse", "PostToolUseFailure", "PermissionDenied"] {
            let p = new_pending();
            let rx = insert_req(&p, 9, "s1", Some("toolu_x"));
            let (_, denied) = drain_pending_on_activity(&p, &ev(name, "s1", Some("toolu_x")));
            assert_eq!(denied, vec![9], "{name}");
            assert_eq!(decision_of(&rx.blocking_recv().unwrap()), "deny");
        }

        let (_, denied) =
            drain_pending_on_activity(&pending, &ev("PostToolUse", "s1", Some("toolu_a")));
        assert_eq!(denied, vec![1]);
        assert_eq!(decision_of(&rx1.blocking_recv().unwrap()), "deny");
        assert!(pending.lock().unwrap().contains_key(&2));
        drop(rx2);

        // 完成事件不带 tool_use_id → 不做 deny（但 PostToolUse 同时是活动事件，orphan 仍 allow）
        let pending = new_pending();
        let rx_tracked = insert_req(&pending, 1, "s1", Some("toolu_a"));
        let (allowed, denied) = drain_pending_on_activity(&pending, &ev("PostToolUse", "s1", None));
        assert!(allowed.is_empty() && denied.is_empty());
        assert!(pending.lock().unwrap().contains_key(&1));
        drop(rx_tracked);
    }

    #[test]
    fn duplicate_tool_use_id_denies_old_waiter() {
        let pending = new_pending();
        let rx_old = insert_req(&pending, 1, "s1", Some("toolu_a"));
        let rx_other = insert_req(&pending, 2, "s1", Some("toolu_b"));

        let old = deny_duplicate_tool_use(&pending, "toolu_a");
        assert_eq!(old, Some(1));
        assert_eq!(decision_of(&rx_old.blocking_recv().unwrap()), "deny");
        assert!(pending.lock().unwrap().contains_key(&2));
        drop(rx_other);

        // 无重复 → None
        assert_eq!(deny_duplicate_tool_use(&pending, "toolu_none"), None);
        // 无 tool_use_id 的请求不参与去重
        let pending = new_pending();
        let _rx = insert_req(&pending, 1, "s1", None);
        assert_eq!(deny_duplicate_tool_use(&pending, "toolu_a"), None);
        assert!(pending.lock().unwrap().contains_key(&1));
    }

    #[test]
    fn resolve_pending_first_answer_wins() {
        let pending = new_pending();
        let rx = insert_req(&pending, 1, "s1", None);
        assert!(resolve_pending(&pending, 1, "allow"));
        assert_eq!(decision_of(&rx.blocking_recv().unwrap()), "allow");
        // 第二次无 waiter，幂等忽略
        assert!(!resolve_pending(&pending, 1, "deny"));
    }
}
