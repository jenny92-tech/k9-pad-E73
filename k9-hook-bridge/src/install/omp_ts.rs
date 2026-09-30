// INPUT:  pi_ts::PI_EXTENSION_TS（编译期常量）
// OUTPUT: omp_extension_ts() — 写入 ~/.omp/agent/extensions/k9pad.ts 的 OMP 扩展源码
// POS:    OMP 扩展模板（适配自蓝本 codeisland-omp.ts）：与 Pi 扩展同一 socket 桥，
//         仅 import 包路径不同（OMP 包 scope），由 Pi 模板替换生成，单一事实源

use super::pi_ts::PI_EXTENSION_TS;

/// OMP 扩展源码：Pi 模板 + 蓝本 codeisland-omp.ts 的 import 差异
/// （OMP 包 scope 的 ExtensionAPI；无 pi-ai 的 AssistantMessage 类型导入）。
/// `_source: "pi"` 与 `pi-` session 前缀与蓝本保持一致（OMP 复用 pi 协议）。
pub fn omp_extension_ts() -> String {
    PI_EXTENSION_TS
        .replacen(
            "// K9-Pad pi extension\n// version: v1",
            "// K9-Pad pi extension\n// version: v1\n// OMP-compatible install",
            1,
        )
        .replacen(
            " * @fileoverview K9-Pad Integration Extension.",
            " * @fileoverview K9-Pad Integration Extension for Oh My Pi / OMP.\n *\n * This is the same socket bridge as the Pi extension, but imports OMP's\n * package scope so `omp` can load it from ~/.omp/agent/extensions.",
            1,
        )
        .replacen(
            "import type { AssistantMessage } from \"@earendil-works/pi-ai\";\nimport type { ExtensionAPI } from \"@earendil-works/pi-coding-agent\";",
            "import type { ExtensionAPI } from \"@oh-my-pi/pi-coding-agent/extensibility/extensions/types\";",
            1,
        )
        .replacen(
            "(m): m is AssistantMessage =>",
            "(m): m is { role: \"assistant\"; content: unknown } =>",
            1,
        )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn omp_template_applies_all_import_differences() {
        let omp = omp_extension_ts();
        // 四处替换全部命中（未命中时输出会残留 Pi 痕迹）
        assert!(omp.contains("// OMP-compatible install"));
        assert!(omp.contains("Oh My Pi / OMP"));
        assert!(omp.contains(
            "import type { ExtensionAPI } from \"@oh-my-pi/pi-coding-agent/extensibility/extensions/types\";"
        ));
        assert!(omp.contains("(m): m is { role: \"assistant\"; content: unknown } =>"));
        assert!(!omp.contains("@earendil-works"));
        assert!(!omp.contains("m is AssistantMessage"));
        // 协议字段与蓝本一致：OMP 复用 pi source 与 pi- 前缀
        assert!(omp.contains("_source: \"pi\""));
        assert!(omp.contains("pi-${sessionId}"));
        // k9pad 适配点
        assert!(omp.contains("/tmp/k9pad-${userId}.sock"));
        assert!(omp.contains(".k9pad/k9-hook-bridge"));
        assert!(omp.contains("K9PAD_SOCKET_PATH"));
    }
}
