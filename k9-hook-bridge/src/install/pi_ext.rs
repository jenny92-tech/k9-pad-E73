// INPUT:  当前 exe 路径 + pi_ts/omp_ts 扩展模板
// OUTPUT: ~/.pi/agent/extensions/k9pad.ts / ~/.omp/agent/extensions/k9pad.ts
// POS:    Pi/OMP 扩展安装器；扩展直连 socket，仅危险命令审批/ask 提问才
//         shell out 调 bridge，所以这里同时安装 bridge 二进制；写前备份 .bak

use std::path::PathBuf;

use super::{backup_and_write, home_dir, install_bridge_binary, omp_ts, pi_ts};

/// 写扩展到 `<home>/.<cli>/agent/extensions/k9pad.ts`（纯路径拼接，便于测试）。
pub fn extension_path_with(home: &std::path::Path, cli: &str) -> PathBuf {
    home.join(format!(".{cli}"))
        .join("agent")
        .join("extensions")
        .join("k9pad.ts")
}

fn install_extension(cli: &str, source: &str) -> Result<String, String> {
    // 扩展的阻塞请求 shell out 调 bridge，先装二进制
    let installed_bin = install_bridge_binary()?;
    let ext_path = extension_path_with(&home_dir()?, cli);
    backup_and_write(&ext_path, source)?;
    Ok(format!(
        "已安装 {cli} 扩展:\n  bridge: {}\n  extension: {}",
        installed_bin.display(),
        ext_path.display()
    ))
}

/// 安装 Pi 扩展（~/.pi/agent/extensions/k9pad.ts）。
pub fn install_pi() -> Result<String, String> {
    install_extension("pi", pi_ts::PI_EXTENSION_TS)
}

/// 安装 OMP 扩展（~/.omp/agent/extensions/k9pad.ts）。
pub fn install_omp() -> Result<String, String> {
    install_extension("omp", &omp_ts::omp_extension_ts())
}

/// 内部校验：扩展模板的关键适配点存在（安装时防御性检查；单测覆盖模板内容）。
#[allow(dead_code)]
fn sanity_check(source: &str) -> bool {
    source.contains("/tmp/k9pad-") && source.contains(".k9pad/k9-hook-bridge")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extension_path_layout() {
        let home = std::path::Path::new("/home/u");
        assert_eq!(
            extension_path_with(home, "pi"),
            PathBuf::from("/home/u/.pi/agent/extensions/k9pad.ts")
        );
        assert_eq!(
            extension_path_with(home, "omp"),
            PathBuf::from("/home/u/.omp/agent/extensions/k9pad.ts")
        );
    }

    #[test]
    fn pi_template_contains_k9pad_adaptations_and_core_logic() {
        let ts = pi_ts::PI_EXTENSION_TS;
        // k9pad 适配点
        assert!(ts.contains("process.env.K9PAD_SOCKET_PATH || `/tmp/k9pad-${userId}.sock`"));
        assert!(ts.contains("`${homedir()}/.k9pad/k9-hook-bridge`"));
        assert!(ts.contains("_source: \"pi\""));
        assert!(!ts.contains("codeisland"));
        assert!(!ts.contains("CodeIsland"));
        // 蓝本核心逻辑保留
        for needle in [
            "DANGEROUS_PATTERNS",
            "pendingPermissionSessions",
            "86_400_000", // ask 工具 24h
            "timeoutMs = 30_000", // 危险命令默认 30s
            "session_before_compact",
            "hook_event_name: \"PermissionRequest\"",
            "tool_name: \"AskUserQuestion\"",
        ] {
            assert!(ts.contains(needle), "missing: {needle}");
        }
        // raw string 安全性：不含终止序列
        assert!(!ts.contains("\"#"));
    }

    #[test]
    fn sanity_check_passes_for_both_templates() {
        assert!(sanity_check(pi_ts::PI_EXTENSION_TS));
        assert!(sanity_check(&omp_ts::omp_extension_ts()));
    }
}
