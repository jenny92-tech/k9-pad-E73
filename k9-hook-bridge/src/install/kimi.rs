// INPUT:  当前 exe 路径 + ~/.kimi-code/config.toml（legacy ~/.kimi/config.toml 存在且 modern 不存在时用 legacy）
// OUTPUT: config.toml 追加 [[hooks]] TOML 块（event/command/timeout[/matcher]）
// POS:    Kimi Code CLI hook 安装器；toml_edit 保留注释格式，已有 k9 块先删（幂等），
//         写前备份 .bak，损坏 TOML 或标量 hooks 冲突拒绝覆盖

use std::fs;
use std::path::PathBuf;

use toml_edit::{ArrayOfTables, DocumentMut, Item, Table};

use super::{backup_and_write, home_dir, install_bridge_binary, is_k9_command};

/// Kimi Code 事件表（对照蓝本 ConfigInstaller.swift 的 Kimi 配置）：
/// 无 PermissionRequest；Notification timeout 上限 600；
/// PreToolUse/PostToolUse/PostToolUseFailure 带 matcher = ".*"。
/// 格式：(event, timeout, 是否带 matcher)
pub const KIMI_EVENTS: &[(&str, u64, bool)] = &[
    ("UserPromptSubmit", 5, false),
    ("PreToolUse", 5, true),
    ("PostToolUse", 5, true),
    ("PostToolUseFailure", 5, true),
    ("Stop", 5, false),
    ("SubagentStart", 5, false),
    ("SubagentStop", 5, false),
    ("SessionStart", 5, false),
    ("SessionEnd", 5, false),
    ("Notification", 600, false),
    ("PreCompact", 5, false),
];

/// Kimi 配置目录解析（纯函数便于测试）：modern（~/.kimi-code）存在优先；
/// 否则 legacy（~/.kimi）的 config.toml 存在用 legacy；都没有默认 modern。
pub fn kimi_home_with(home: &std::path::Path) -> PathBuf {
    let modern = home.join(".kimi-code");
    let legacy = home.join(".kimi");
    if modern.exists() {
        modern
    } else if legacy.join("config.toml").exists() {
        legacy
    } else {
        modern
    }
}

fn kimi_config_path() -> Result<PathBuf, String> {
    Ok(kimi_home_with(&home_dir()?).join("config.toml"))
}

/// 把 k9 的 [[hooks]] 块合并进 config.toml 文本（纯函数，可测）。
/// 语义：command 含 k9 二进制的旧块先删再追加；其他 [[hooks]] 块与注释原样保留；
/// 已有标量形式的 `hooks = ...` 与数组冲突，拒绝覆盖。
pub fn merge_kimi_hooks(text: &str, command: &str) -> Result<String, String> {
    let mut doc: DocumentMut = text
        .parse()
        .map_err(|e| format!("config.toml 不是合法 TOML，拒绝覆盖: {e}"))?;

    if let Some(item) = doc.get("hooks") {
        if item.as_array_of_tables().is_none() {
            return Err(
                "config.toml 已有非 [[hooks]] 形式的 hooks 配置（标量/数组冲突），拒绝覆盖"
                    .to_string(),
            );
        }
    }
    if doc.get("hooks").is_none() {
        doc["hooks"] = Item::ArrayOfTables(ArrayOfTables::new());
    }
    let hooks = doc["hooks"]
        .as_array_of_tables_mut()
        .ok_or_else(|| "config.toml 的 hooks 不是 [[hooks]] 数组，拒绝覆盖".to_string())?;

    // 幂等：先删 command 含 k9 二进制的旧块（倒序删避免索引漂移）
    let stale: Vec<usize> = hooks
        .iter()
        .enumerate()
        .filter_map(|(i, t)| {
            t.get("command")
                .and_then(Item::as_str)
                .filter(|c| is_k9_command(c))
                .map(|_| i)
        })
        .collect();
    for i in stale.into_iter().rev() {
        hooks.remove(i);
    }

    for (event, timeout, with_matcher) in KIMI_EVENTS {
        let mut t = Table::new();
        t["event"] = toml_edit::value(*event);
        t["command"] = toml_edit::value(command);
        t["timeout"] = toml_edit::value(*timeout as i64);
        if *with_matcher {
            t["matcher"] = toml_edit::value(".*");
        }
        hooks.push(t);
    }
    Ok(doc.to_string())
}

/// 安装：复制自身到 ~/.k9pad/k9-hook-bridge，合并写 Kimi config.toml。
pub fn install_kimi() -> Result<String, String> {
    let installed_bin = install_bridge_binary()?;
    let command = format!("{} --source kimi", installed_bin.display());
    let config_path = kimi_config_path()?;

    let old = match fs::read_to_string(&config_path) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(e) => return Err(format!("读取 {} 失败: {e}", config_path.display())),
    };
    let new = merge_kimi_hooks(&old, &command)?;
    if new != old {
        backup_and_write(&config_path, &new)?;
    }

    Ok(format!(
        "已安装 Kimi Code hooks:\n  bridge: {}\n  config: {}",
        installed_bin.display(),
        config_path.display()
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    const CMD: &str = "/Users/x/.k9pad/k9-hook-bridge --source kimi";

    fn k9_block_count(out: &str) -> usize {
        let doc: DocumentMut = out.parse().unwrap();
        doc["hooks"]
            .as_array_of_tables()
            .unwrap()
            .iter()
            .filter(|t| t["command"].as_str().map(is_k9_command).unwrap_or(false))
            .count()
    }

    #[test]
    fn merge_into_empty_creates_all_blocks() {
        let out = merge_kimi_hooks("", CMD).unwrap();
        let doc: DocumentMut = out.parse().unwrap();
        let hooks = doc["hooks"].as_array_of_tables().unwrap();
        assert_eq!(hooks.len(), KIMI_EVENTS.len());

        let first = hooks.get(0).unwrap();
        assert_eq!(first["event"].as_str(), Some("UserPromptSubmit"));
        assert_eq!(first["command"].as_str(), Some(CMD));
        assert_eq!(first["timeout"].as_integer(), Some(5));
        assert!(first.get("matcher").is_none());

        let pre = hooks.get(1).unwrap();
        assert_eq!(pre["event"].as_str(), Some("PreToolUse"));
        assert_eq!(pre["matcher"].as_str(), Some(".*"));

        let notif = hooks
            .iter()
            .find(|t| t["event"].as_str() == Some("Notification"))
            .unwrap();
        assert_eq!(notif["timeout"].as_integer(), Some(600));
    }

    #[test]
    fn merge_preserves_existing_config_and_other_hook_blocks() {
        let input = "# 我的配置\nmodel = \"k2\"\n\n[[hooks]]\nevent = \"Stop\"\ncommand = \"my-notify\"\ntimeout = 5\n";
        let out = merge_kimi_hooks(input, CMD).unwrap();
        assert!(out.contains("# 我的配置"));
        assert!(out.contains("model = \"k2\""));
        assert!(out.contains("my-notify"));

        let doc: DocumentMut = out.parse().unwrap();
        let hooks = doc["hooks"].as_array_of_tables().unwrap();
        assert_eq!(hooks.len(), KIMI_EVENTS.len() + 1);
        assert_eq!(hooks.get(0).unwrap()["command"].as_str(), Some("my-notify"));
    }

    #[test]
    fn merge_is_idempotent_replacing_stale_k9_blocks() {
        let first = merge_kimi_hooks("", "/old/k9-hook-bridge --source kimi").unwrap();
        let second = merge_kimi_hooks(&first, CMD).unwrap();
        let third = merge_kimi_hooks(&second, CMD).unwrap();
        assert_eq!(k9_block_count(&third), KIMI_EVENTS.len());
        // 再跑一遍输出稳定
        let fourth = merge_kimi_hooks(&third, CMD).unwrap();
        assert_eq!(fourth, third);
    }

    #[test]
    fn merge_rejects_scalar_hooks_and_invalid_toml() {
        assert!(merge_kimi_hooks("hooks = true\n", CMD).is_err());
        assert!(merge_kimi_hooks("= = not toml", CMD).is_err());
    }
}
