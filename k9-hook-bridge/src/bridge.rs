// INPUT:  原始 hook 事件 JSON（serde_json::Value）+ source/ppid/cwd 兜底值
// OUTPUT: enrich_event 字段注入、is_blocking 阻塞性判定、session_id 兜底（纯函数）
// POS:    桥接流程的可测纯逻辑层；被 main.rs 编排调用，被单测直接覆盖

use serde_json::{Map, Value};

/// 注入富化字段（对照蓝本 CodeIslandBridge/main.swift）：
/// - `_source`：payload 已带非空 `_source` 时以 payload 为准（Pi/OMP 扩展直接发
///   `_source:"pi"`，bridge 无 argv 调用）；否则注入 argv 传入的来源标记
/// - `_ppid`：直接父进程 pid
/// - `cwd`：缺失时补当前目录
/// - `session_id`：缺失且有 source（payload 优先，其次 argv）时兜底 `<source>-ppid-<pid>`
pub fn enrich_event(json: &mut Map<String, Value>, source: Option<&str>, ppid: u32, cwd: &str) {
    let payload_source = json
        .get("_source")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .map(str::to_string);
    if payload_source.is_none() {
        if let Some(source) = source {
            json.insert("_source".to_string(), Value::String(source.to_string()));
        }
    }
    json.insert("_ppid".to_string(), Value::Number(ppid.into()));

    if !json.contains_key("cwd") {
        json.insert("cwd".to_string(), Value::String(cwd.to_string()));
    }

    let effective_source = payload_source.as_deref().or(source);
    if !has_valid_session_id(json) {
        if let Some(source) = effective_source {
            if !source.trim().is_empty() {
                json.insert(
                    "session_id".to_string(),
                    Value::String(format!("{source}-ppid-{ppid}")),
                );
            }
        }
    }
}

/// session_id 必须是非空字符串才算有效。
pub fn has_valid_session_id(json: &Map<String, Value>) -> bool {
    json.get("session_id")
        .and_then(Value::as_str)
        .map(|s| !s.is_empty())
        .unwrap_or(false)
}

/// 阻塞事件判定：需要等待用户决策、server 会回响应的事件。
/// - `hook_event_name == "PermissionRequest"`
/// - `hook_event_name == "Notification"` 且 payload 含 `question` 字段
pub fn is_blocking(json: &Map<String, Value>) -> bool {
    let event = json.get("hook_event_name").and_then(Value::as_str);
    match event {
        Some("PermissionRequest") => true,
        Some("Notification") => json.get("question").map(Value::is_string).unwrap_or(false),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn obj(v: Value) -> Map<String, Value> {
        v.as_object().unwrap().clone()
    }

    #[test]
    fn enrich_injects_source_ppid_and_keeps_original_fields() {
        let mut json = obj(json!({
            "session_id": "s1",
            "hook_event_name": "SessionStart",
            "cwd": "/tmp",
            "tool_name": "Bash"
        }));
        enrich_event(&mut json, Some("claude"), 4242, "/fallback");

        assert_eq!(json.get("_source").unwrap(), &json!("claude"));
        assert_eq!(json.get("_ppid").unwrap(), &json!(4242));
        assert_eq!(json.get("cwd").unwrap(), &json!("/tmp"));
        assert_eq!(json.get("session_id").unwrap(), &json!("s1"));
        assert_eq!(json.get("tool_name").unwrap(), &json!("Bash"));
    }

    #[test]
    fn enrich_fills_cwd_only_when_missing() {
        let mut with_cwd = obj(json!({"session_id": "s1", "cwd": "/given"}));
        enrich_event(&mut with_cwd, None, 1, "/fallback");
        assert_eq!(with_cwd.get("cwd").unwrap(), &json!("/given"));

        let mut without_cwd = obj(json!({"session_id": "s1"}));
        enrich_event(&mut without_cwd, None, 1, "/fallback");
        assert_eq!(without_cwd.get("cwd").unwrap(), &json!("/fallback"));
    }

    #[test]
    fn enrich_falls_back_session_id_only_with_source() {
        let mut json = obj(json!({"hook_event_name": "Stop"}));
        enrich_event(&mut json, Some("claude"), 99, "/tmp");
        assert_eq!(json.get("session_id").unwrap(), &json!("claude-ppid-99"));

        // 空 session_id 字符串同样触发兜底
        let mut empty = obj(json!({"session_id": ""}));
        enrich_event(&mut empty, Some("codex"), 7, "/tmp");
        assert_eq!(empty.get("session_id").unwrap(), &json!("codex-ppid-7"));

        // 无 source：不兜底，保持缺失（调用方据此静默退出）
        let mut no_source = obj(json!({"hook_event_name": "Stop"}));
        enrich_event(&mut no_source, None, 99, "/tmp");
        assert!(!no_source.contains_key("session_id"));
    }

    #[test]
    fn payload_source_wins_over_argv_and_drives_session_fallback() {
        // payload 已带 _source：以 payload 为准，argv 不覆盖
        let mut json = obj(json!({"session_id": "s1", "_source": "pi"}));
        enrich_event(&mut json, Some("claude"), 42, "/tmp");
        assert_eq!(json.get("_source").unwrap(), &json!("pi"));

        // argv 缺省：session_id 兜底用 payload 的 _source（Pi 扩展直发场景）
        let mut json = obj(json!({"hook_event_name": "Stop", "_source": "pi"}));
        enrich_event(&mut json, None, 42, "/tmp");
        assert_eq!(json.get("session_id").unwrap(), &json!("pi-ppid-42"));

        // payload _source 为空串：视为缺失，argv 注入
        let mut json = obj(json!({"session_id": "s1", "_source": ""}));
        enrich_event(&mut json, Some("codex"), 42, "/tmp");
        assert_eq!(json.get("_source").unwrap(), &json!("codex"));

        // 两者都缺：不兜底，保持缺失（调用方据此静默退出）
        let mut json = obj(json!({"hook_event_name": "Stop"}));
        enrich_event(&mut json, None, 42, "/tmp");
        assert!(!json.contains_key("session_id"));
        assert!(!json.contains_key("_source"));
    }

    #[test]
    fn blocking_decision_table() {
        // (事件 payload, 期望是否阻塞)
        let cases = [
            (json!({"hook_event_name": "PermissionRequest"}), true),
            (
                json!({"hook_event_name": "Notification", "question": "继续吗？"}),
                true,
            ),
            (json!({"hook_event_name": "Notification"}), false),
            // question 不是字符串不算提问
            (
                json!({"hook_event_name": "Notification", "question": 1}),
                false,
            ),
            (json!({"hook_event_name": "SessionStart"}), false),
            (json!({"hook_event_name": "PreToolUse"}), false),
            (json!({"hook_event_name": "Stop"}), false),
            (json!({}), false),
        ];
        for (payload, expected) in cases {
            let desc = payload.to_string();
            assert_eq!(is_blocking(&obj(payload)), expected, "payload: {desc}");
        }
    }
}
