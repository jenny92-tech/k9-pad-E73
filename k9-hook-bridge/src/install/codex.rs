// INPUT:  当前 exe 路径 + $CODEX_HOME（或 ~/.codex）下已有 hooks.json / config.toml
// OUTPUT: ~/.codex/hooks.json 的 hooks 节（nested 无 matcher 格式）+ config.toml [features] hooks = true
// POS:    Codex CLI hook 安装器；只动 hooks 节与 [features] 的 hooks 开关，
//         其他 key/注释用 toml_edit 保留，写前备份 .bak，损坏文件拒绝覆盖

use std::fs;
use std::path::PathBuf;

use serde_json::{json, Map, Value};
use toml_edit::{DocumentMut, Item, Table};

use super::{backup_and_write, home_dir, install_bridge_binary, is_k9_command};

/// Codex 事件表（对照蓝本 ConfigInstaller.swift 的 Codex 配置）：
/// nested 格式无 matcher；PermissionRequest 阻塞 timeout 86400，SessionEnd 3s，其余 5s。
pub const CODEX_EVENTS: &[(&str, u64)] = &[
    ("SessionStart", 5),
    ("SessionEnd", 3),
    ("UserPromptSubmit", 5),
    ("PreToolUse", 5),
    ("PostToolUse", 5),
    ("PermissionRequest", 86400),
    ("Stop", 5),
];

/// Codex 配置目录解析（纯函数便于测试）：$CODEX_HOME（空白视为未设置，前导 ~ 展开）
/// 优先，缺省回退 `<home>/.codex`。
pub fn codex_home_with(home: &std::path::Path, env: Option<&str>) -> PathBuf {
    if let Some(raw) = env {
        let trimmed = raw.trim();
        if !trimmed.is_empty() {
            if trimmed == "~" {
                return home.to_path_buf();
            }
            if let Some(rest) = trimmed.strip_prefix("~/") {
                return home.join(rest);
            }
            return PathBuf::from(trimmed);
        }
    }
    home.join(".codex")
}

fn codex_home() -> Result<PathBuf, String> {
    let home = home_dir()?;
    let env = std::env::var("CODEX_HOME").ok();
    Ok(codex_home_with(&home, env.as_deref()))
}

/// 把 k9 的 hooks 合并进 Codex hooks.json（纯函数，可测）。
/// 语义与 Claude 安装器一致：保留顶层其他 key 与每事件下其他工具 entry；
/// 已存在的 k9 entry 先删再插；唯一差别是 nested 格式（entry 无 matcher）。
pub fn merge_codex_hooks(settings: &mut Value, command: &str) -> Result<(), String> {
    let root = settings
        .as_object_mut()
        .ok_or_else(|| "hooks.json 顶层不是 JSON object，拒绝覆盖".to_string())?;
    if !root.contains_key("hooks") {
        root.insert("hooks".to_string(), Value::Object(Map::new()));
    }
    let hooks = root
        .get_mut("hooks")
        .and_then(Value::as_object_mut)
        .ok_or_else(|| "hooks.json 的 hooks 不是 JSON object，拒绝覆盖".to_string())?;

    for (event, timeout) in CODEX_EVENTS {
        if !hooks.contains_key(*event) {
            hooks.insert(event.to_string(), Value::Array(Vec::new()));
        }
        let entries = hooks
            .get_mut(*event)
            .and_then(Value::as_array_mut)
            .ok_or_else(|| format!("hooks.{event} 不是 array，拒绝覆盖"))?;
        entries.retain(|entry| {
            !entry
                .get("hooks")
                .and_then(Value::as_array)
                .map(|hooks| {
                    hooks.iter().any(|h| {
                        h.get("command")
                            .and_then(Value::as_str)
                            .map(is_k9_command)
                            .unwrap_or(false)
                    })
                })
                .unwrap_or(false)
        });
        entries.push(json!({
            "hooks": [{
                "type": "command",
                "command": command,
                "timeout": timeout,
            }]
        }));
    }
    Ok(())
}

/// 确保 config.toml 的 [features] 节有 hooks = true（纯函数，可测）。
/// 旧名 codex_hooks 存在时改写为 hooks（当前名缺失才改写，否则只删旧名）；
/// hooks = false 原地翻转为 true；toml_edit 保留注释与格式。损坏 TOML 拒绝覆盖。
pub fn ensure_codex_hooks_feature(text: &str) -> Result<String, String> {
    let mut doc: DocumentMut = text
        .parse()
        .map_err(|e| format!("config.toml 不是合法 TOML，拒绝覆盖: {e}"))?;

    if doc.get("features").is_none() {
        doc["features"] = Item::Table(Table::new());
    }
    let features = doc["features"]
        .as_table_like_mut()
        .ok_or_else(|| "config.toml 的 features 不是 table，拒绝覆盖".to_string())?;

    // 旧名 codex_hooks：当前名缺失则改写为 hooks = true，否则仅删除旧名
    if features.remove("codex_hooks").is_some() && features.get("hooks").is_none() {
        features.insert("hooks", toml_edit::value(true));
    }
    if features.get("hooks").and_then(Item::as_bool) != Some(true) {
        features.insert("hooks", toml_edit::value(true));
    }
    Ok(doc.to_string())
}

/// 安装：复制自身到 ~/.k9pad/k9-hook-bridge，合并写 hooks.json + 确保 config.toml 开关。
pub fn install_codex() -> Result<String, String> {
    let installed_bin = install_bridge_binary()?;
    let command = format!("{} --source codex", installed_bin.display());
    let codex_dir = codex_home()?;

    // 1. hooks.json（损坏 JSON 拒绝覆盖）
    let hooks_path = codex_dir.join("hooks.json");
    let mut settings = match fs::read_to_string(&hooks_path) {
        Ok(text) if !text.trim().is_empty() => serde_json::from_str::<Value>(&text)
            .map_err(|e| format!("{} 不是合法 JSON，拒绝覆盖: {e}", hooks_path.display()))?,
        Ok(_) => json!({}),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => json!({}),
        Err(e) => return Err(format!("读取 {} 失败: {e}", hooks_path.display())),
    };
    merge_codex_hooks(&mut settings, &command)?;
    let text = serde_json::to_string_pretty(&settings)
        .map_err(|e| format!("序列化 hooks.json 失败: {e}"))?;
    backup_and_write(&hooks_path, &format!("{text}\n"))?;

    // 2. config.toml：[features] hooks = true（内容未变化时不重写，避免无谓的 .bak）
    let config_path = codex_dir.join("config.toml");
    let old = match fs::read_to_string(&config_path) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(e) => return Err(format!("读取 {} 失败: {e}", config_path.display())),
    };
    let new = ensure_codex_hooks_feature(&old)?;
    if new != old {
        backup_and_write(&config_path, &new)?;
    }

    Ok(format!(
        "已安装 Codex hooks:\n  bridge: {}\n  hooks: {}\n  config: {}",
        installed_bin.display(),
        hooks_path.display(),
        config_path.display()
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    const CMD: &str = "/Users/x/.k9pad/k9-hook-bridge --source codex";

    fn k9_entries(settings: &Value, event: &str) -> usize {
        settings["hooks"][event]
            .as_array()
            .unwrap()
            .iter()
            .filter(|e| {
                e["hooks"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|h| h["command"].as_str().map(is_k9_command).unwrap_or(false))
            })
            .count()
    }

    #[test]
    fn merge_into_empty_object() {
        let mut settings = json!({});
        merge_codex_hooks(&mut settings, CMD).unwrap();

        let hooks = settings["hooks"].as_object().unwrap();
        assert_eq!(hooks.len(), CODEX_EVENTS.len());
        // nested 格式：无 matcher
        let entry = &hooks["PermissionRequest"][0];
        assert!(entry.get("matcher").is_none());
        assert_eq!(entry["hooks"][0]["type"], json!("command"));
        assert_eq!(entry["hooks"][0]["command"], json!(CMD));
        assert_eq!(entry["hooks"][0]["timeout"], json!(86400));
        assert_eq!(hooks["SessionEnd"][0]["hooks"][0]["timeout"], json!(3));
        assert_eq!(hooks["PreToolUse"][0]["hooks"][0]["timeout"], json!(5));
    }

    #[test]
    fn merge_preserves_other_keys_and_other_tools_hooks() {
        let mut settings = json!({
            "other": 1,
            "hooks": {
                "PreToolUse": [
                    {"hooks": [{"type": "command", "command": "my-hook", "timeout": 5}]}
                ]
            }
        });
        merge_codex_hooks(&mut settings, CMD).unwrap();

        assert_eq!(settings["other"], json!(1));
        let pre = settings["hooks"]["PreToolUse"].as_array().unwrap();
        assert_eq!(pre.len(), 2);
        assert_eq!(pre[0]["hooks"][0]["command"], json!("my-hook"));
        assert_eq!(pre[1]["hooks"][0]["command"], json!(CMD));
    }

    #[test]
    fn merge_is_idempotent_replacing_stale_k9_entries() {
        let mut settings = json!({});
        merge_codex_hooks(&mut settings, "/old/k9-hook-bridge --source codex").unwrap();
        merge_codex_hooks(&mut settings, CMD).unwrap();
        merge_codex_hooks(&mut settings, CMD).unwrap();

        for (event, _) in CODEX_EVENTS {
            assert_eq!(k9_entries(&settings, event), 1, "event: {event}");
        }
    }

    #[test]
    fn merge_rejects_non_object_settings_and_hooks() {
        let mut not_obj = json!([]);
        assert!(merge_codex_hooks(&mut not_obj, CMD).is_err());
        let mut hooks_not_obj = json!({"hooks": "nope"});
        assert!(merge_codex_hooks(&mut hooks_not_obj, CMD).is_err());
    }

    #[test]
    fn features_created_in_empty_config() {
        let out = ensure_codex_hooks_feature("").unwrap();
        assert_eq!(out, "[features]\nhooks = true\n");
    }

    #[test]
    fn features_inserted_into_existing_section_preserving_comments() {
        let input = "# 用户注释\nmodel = \"gpt-5\"\n\n[features]\n# 已有开关\nother = true\n";
        let out = ensure_codex_hooks_feature(input).unwrap();
        assert!(out.contains("# 用户注释"));
        assert!(out.contains("# 已有开关"));
        assert!(out.contains("other = true"));
        assert!(out.contains("hooks = true"));
        // 幂等：再跑一遍输出不变
        let again = ensure_codex_hooks_feature(&out).unwrap();
        assert_eq!(again, out);
    }

    #[test]
    fn hooks_false_flipped_to_true() {
        let out = ensure_codex_hooks_feature("[features]\nhooks = false\n").unwrap();
        assert!(out.contains("hooks = true"));
        assert!(!out.contains("hooks = false"));
    }

    #[test]
    fn legacy_codex_hooks_renamed() {
        // 当前名缺失：旧名改写为 hooks = true
        let out = ensure_codex_hooks_feature("[features]\ncodex_hooks = true\n").unwrap();
        assert!(out.contains("hooks = true"));
        assert!(!out.contains("codex_hooks"));

        // 当前名已存在：仅删旧名，当前名保持
        let out = ensure_codex_hooks_feature("[features]\nhooks = false\ncodex_hooks = true\n")
            .unwrap();
        assert!(out.contains("hooks = true"));
        assert!(!out.contains("codex_hooks"));
    }

    #[test]
    fn rejects_invalid_toml_and_non_table_features() {
        assert!(ensure_codex_hooks_feature("= = = not toml").is_err());
        assert!(ensure_codex_hooks_feature("features = 1\n").is_err());
    }

    #[test]
    fn codex_home_resolution() {
        let home = std::path::Path::new("/home/u");
        assert_eq!(codex_home_with(home, None), PathBuf::from("/home/u/.codex"));
        assert_eq!(
            codex_home_with(home, Some("  ")),
            PathBuf::from("/home/u/.codex")
        );
        assert_eq!(
            codex_home_with(home, Some("/opt/codex")),
            PathBuf::from("/opt/codex")
        );
        assert_eq!(
            codex_home_with(home, Some("~/mycodex")),
            PathBuf::from("/home/u/mycodex")
        );
        assert_eq!(codex_home_with(home, Some("~")), home.to_path_buf());
    }
}
