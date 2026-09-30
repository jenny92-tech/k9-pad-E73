// INPUT:  install <claude|codex|kimi|pi|omp> 参数 + 当前 exe 路径 + 已有 CLI 配置文件
// OUTPUT: ~/.k9pad/k9-hook-bridge 二进制副本 + 各 CLI 的 hooks 安装物（合并写入）
// POS:    安装器公共层：二进制安装、写前 .bak 备份、幂等合并语义；
//         各 CLI 的具体格式在 codex/kimi/pi_ext 子模块，只动 hooks 节，其他内容原样保留

use std::fs;
use std::path::{Path, PathBuf};

use serde_json::{json, Map, Value};

pub mod codex;
pub mod kimi;
pub mod omp_ts;
pub mod pi_ext;
pub mod pi_ts;

/// Claude Code 事件表（对照蓝本 ConfigInstaller.swift 的 Claude 配置）：
/// PermissionRequest / Notification 是阻塞事件，timeout 86400；其余 5s。
pub const CLAUDE_EVENTS: &[(&str, u64)] = &[
    ("UserPromptSubmit", 5),
    ("PreToolUse", 5),
    ("PostToolUse", 5),
    ("PostToolUseFailure", 5),
    ("PermissionRequest", 86400),
    ("Stop", 5),
    ("SubagentStart", 5),
    ("SubagentStop", 5),
    ("SessionStart", 5),
    ("SessionEnd", 5),
    ("Notification", 86400),
    ("PreCompact", 5),
];

const INSTALL_DIR: &str = ".k9pad";
const BRIDGE_BIN: &str = "k9-hook-bridge";

pub(crate) fn home_dir() -> Result<PathBuf, String> {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .filter(|p| !p.as_os_str().is_empty())
        .ok_or_else(|| "无法确定 HOME 目录".to_string())
}

/// 复制当前 exe 到 ~/.k9pad/k9-hook-bridge，返回安装后路径（各安装器共用）。
pub(crate) fn install_bridge_binary() -> Result<PathBuf, String> {
    let install_dir = home_dir()?.join(INSTALL_DIR);
    fs::create_dir_all(&install_dir)
        .map_err(|e| format!("创建 {} 失败: {e}", install_dir.display()))?;
    let current_exe =
        std::env::current_exe().map_err(|e| format!("获取当前 exe 路径失败: {e}"))?;
    let installed_bin = install_dir.join(BRIDGE_BIN);
    fs::copy(&current_exe, &installed_bin)
        .map_err(|e| format!("复制 {} 失败: {e}", installed_bin.display()))?;
    Ok(installed_bin)
}

/// 写前备份 `<file>.bak`（已存在时）+ 写入（父目录自动创建）。各安装器共用。
pub(crate) fn backup_and_write(path: &Path, contents: &str) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| format!("创建 {} 失败: {e}", parent.display()))?;
    }
    if path.exists() {
        let file_name = path
            .file_name()
            .ok_or_else(|| format!("{} 不是文件路径", path.display()))?
            .to_string_lossy();
        let backup = path.with_file_name(format!("{file_name}.bak"));
        fs::copy(path, &backup).map_err(|e| format!("备份到 {} 失败: {e}", backup.display()))?;
    }
    fs::write(path, contents).map_err(|e| format!("写入 {} 失败: {e}", path.display()))
}

/// settings.json 路径解析：--config-dir > $CLAUDE_CONFIG_DIR > ~/.claude
fn claude_settings_path(config_dir: Option<PathBuf>) -> Result<PathBuf, String> {
    let dir = match config_dir {
        Some(dir) => dir,
        None => match std::env::var_os("CLAUDE_CONFIG_DIR") {
            Some(dir) if !dir.is_empty() => PathBuf::from(dir),
            _ => home_dir()?.join(".claude"),
        },
    };
    Ok(dir.join("settings.json"))
}

/// 判断一条 hook command 是否由 k9pad 安装（幂等删除的依据）。
pub(crate) fn is_k9_command(command: &str) -> bool {
    command.contains(BRIDGE_BIN)
}

/// 判断 hooks 数组里的一条 entry 是否包含 k9 的 hook。
fn is_k9_entry(entry: &Value) -> bool {
    entry
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
}

/// 把 k9 的 hooks 合并进 settings（纯函数，可测）。
/// 语义：保留顶层其他 key 与每个事件下其他工具的 entry；已存在的 k9 entry 先删再插。
pub fn merge_claude_hooks(settings: &mut Value, command: &str) -> Result<(), String> {
    let root = settings
        .as_object_mut()
        .ok_or_else(|| "settings.json 顶层不是 JSON object，拒绝覆盖".to_string())?;
    if !root.contains_key("hooks") {
        root.insert("hooks".to_string(), Value::Object(Map::new()));
    }
    let hooks = root
        .get_mut("hooks")
        .and_then(Value::as_object_mut)
        .ok_or_else(|| "settings.json 的 hooks 不是 JSON object，拒绝覆盖".to_string())?;

    for (event, timeout) in CLAUDE_EVENTS {
        if !hooks.contains_key(*event) {
            hooks.insert(event.to_string(), Value::Array(Vec::new()));
        }
        let entries = hooks
            .get_mut(*event)
            .and_then(Value::as_array_mut)
            .ok_or_else(|| format!("hooks.{event} 不是 array，拒绝覆盖"))?;
        entries.retain(|entry| !is_k9_entry(entry));
        entries.push(json!({
            "matcher": "",
            "hooks": [{
                "type": "command",
                "command": command,
                "timeout": timeout,
            }]
        }));
    }
    Ok(())
}

/// 安装：复制自身到 ~/.k9pad/k9-hook-bridge，合并写 Claude settings.json。
/// 与桥接模式不同——安装失败显式报错（调用方非零退出）。
pub fn install_claude(config_dir: Option<PathBuf>) -> Result<String, String> {
    // 1. 安装 bridge 二进制
    let installed_bin = install_bridge_binary()?;
    let command = format!("{} --source claude", installed_bin.display());

    // 2. 读已有 settings（损坏 JSON 拒绝覆盖，保护用户配置）
    let settings_path = claude_settings_path(config_dir)?;
    let mut settings = match fs::read_to_string(&settings_path) {
        Ok(text) if !text.trim().is_empty() => serde_json::from_str::<Value>(&text)
            .map_err(|e| format!("{} 不是合法 JSON，拒绝覆盖: {e}", settings_path.display()))?,
        Ok(_) => json!({}),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => json!({}),
        Err(e) => return Err(format!("读取 {} 失败: {e}", settings_path.display())),
    };

    // 3. 合并 hooks 节
    merge_claude_hooks(&mut settings, &command)?;

    // 4. 写前备份 + 写入
    let text = serde_json::to_string_pretty(&settings)
        .map_err(|e| format!("序列化 settings 失败: {e}"))?;
    backup_and_write(&settings_path, &format!("{text}\n"))?;

    Ok(format!(
        "已安装 Claude Code hooks:\n  bridge: {}\n  settings: {}",
        installed_bin.display(),
        settings_path.display()
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    const CMD: &str = "/Users/x/.k9pad/k9-hook-bridge --source claude";

    fn k9_entries(settings: &Value, event: &str) -> usize {
        settings["hooks"][event]
            .as_array()
            .unwrap()
            .iter()
            .filter(|e| is_k9_entry(e))
            .count()
    }

    #[test]
    fn merge_into_empty_object() {
        let mut settings = json!({});
        merge_claude_hooks(&mut settings, CMD).unwrap();

        let hooks = settings["hooks"].as_object().unwrap();
        assert_eq!(hooks.len(), CLAUDE_EVENTS.len());
        let entry = &hooks["PermissionRequest"][0];
        assert_eq!(entry["matcher"], json!(""));
        assert_eq!(entry["hooks"][0]["type"], json!("command"));
        assert_eq!(entry["hooks"][0]["command"], json!(CMD));
        assert_eq!(entry["hooks"][0]["timeout"], json!(86400));
        assert_eq!(hooks["PreToolUse"][0]["hooks"][0]["timeout"], json!(5));
    }

    #[test]
    fn merge_preserves_other_keys_and_other_tools_hooks() {
        let mut settings = json!({
            "model": "opus",
            "hooks": {
                "PreToolUse": [
                    {"matcher": "Bash", "hooks": [{"type": "command", "command": "my-linter"}]}
                ],
                "CustomEvent": [{"matcher": "", "hooks": []}]
            }
        });
        merge_claude_hooks(&mut settings, CMD).unwrap();

        assert_eq!(settings["model"], json!("opus"));
        // 其他工具的 entry 仍在，k9 的追加在后
        let pre = settings["hooks"]["PreToolUse"].as_array().unwrap();
        assert_eq!(pre.len(), 2);
        assert_eq!(pre[0]["hooks"][0]["command"], json!("my-linter"));
        assert_eq!(pre[1]["hooks"][0]["command"], json!(CMD));
        // 未管理的事件 key 原样保留
        assert!(settings["hooks"]["CustomEvent"].is_array());
    }

    #[test]
    fn merge_is_idempotent_replacing_stale_k9_entries() {
        let mut settings = json!({});
        merge_claude_hooks(&mut settings, "/old/path/k9-hook-bridge --source claude").unwrap();
        merge_claude_hooks(&mut settings, CMD).unwrap();
        merge_claude_hooks(&mut settings, CMD).unwrap();

        for (event, _) in CLAUDE_EVENTS {
            assert_eq!(k9_entries(&settings, event), 1, "event: {event}");
        }
        assert_eq!(
            settings["hooks"]["Stop"][0]["hooks"][0]["command"],
            json!(CMD)
        );
    }

    #[test]
    fn merge_rejects_non_object_settings_and_hooks() {
        let mut not_obj = json!([]);
        assert!(merge_claude_hooks(&mut not_obj, CMD).is_err());

        let mut hooks_not_obj = json!({"hooks": "nope"});
        assert!(merge_claude_hooks(&mut hooks_not_obj, CMD).is_err());
    }
}
