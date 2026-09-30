// INPUT:  std::fs, serde_json, notch_shape (EndStyle)
// OUTPUT: load() / save() — ~/.k9pad/config.json 的刘海面板开发调参读写
// POS:    配置持久化 — 端部样式/响应式宽度微调/紧凑态圆角落盘，启动时读入 AppState

use crate::notch_shape::EndStyle;

/// 响应式宽度基线的默认调参值；1.8 对应 1.0× 基线，保留旧配置兼容性。
pub const DEFAULT_WIDEN_FACTOR: f32 = 1.8;
/// 默认顶部反角/底部圆角（32pt 行高基准）
pub const DEFAULT_TOP_R: f32 = 6.0;
pub const DEFAULT_BOT_R: f32 = 14.0;

fn config_path() -> std::path::PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".into());
    std::path::Path::new(&home).join(".k9pad/config.json")
}

/// 读取配置（文件缺失/损坏时返回默认值，不影响启动）
pub fn load() -> NotchConfig {
    let Ok(s) = std::fs::read_to_string(config_path()) else {
        return NotchConfig::default();
    };
    let Ok(v) = serde_json::from_str::<serde_json::Value>(&s) else {
        return NotchConfig::default();
    };
    let style = v
        .get("notch_style")
        .and_then(|x| x.as_str())
        .map(EndStyle::from_config_str)
        .unwrap_or_default();
    let factor = v
        .get("widen_factor")
        .and_then(|x| x.as_f64())
        .map(|f| (f as f32).clamp(1.0, 3.0))
        .unwrap_or(DEFAULT_WIDEN_FACTOR);
    let top_r = v
        .get("notch_top_r")
        .and_then(|x| x.as_f64())
        .map(|f| (f as f32).clamp(0.0, 24.0))
        .unwrap_or(DEFAULT_TOP_R);
    let bot_r = v
        .get("notch_bot_r")
        .and_then(|x| x.as_f64())
        .map(|f| (f as f32).clamp(0.0, 24.0))
        .unwrap_or(DEFAULT_BOT_R);
    NotchConfig {
        style,
        widen_factor: factor,
        top_r,
        bot_r,
    }
}

/// 刘海面板配置
#[derive(Debug, Clone, Copy)]
pub struct NotchConfig {
    pub style: EndStyle,
    pub widen_factor: f32,
    pub top_r: f32,
    pub bot_r: f32,
}

impl Default for NotchConfig {
    fn default() -> Self {
        Self {
            style: EndStyle::default(),
            widen_factor: DEFAULT_WIDEN_FACTOR,
            top_r: DEFAULT_TOP_R,
            bot_r: DEFAULT_BOT_R,
        }
    }
}

/// 写回配置（保留文件里其他 key）
pub fn save(style: EndStyle, widen_factor: f32, top_r: f32, bot_r: f32) {
    let path = config_path();
    let mut v = std::fs::read_to_string(&path)
        .ok()
        .and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok())
        .unwrap_or_else(|| serde_json::json!({}));
    let style_str = match style {
        EndStyle::Notch => "notch",
        EndStyle::Capsule => "capsule",
    };
    v["notch_style"] = serde_json::json!(style_str);
    v["widen_factor"] = serde_json::json!(widen_factor);
    v["notch_top_r"] = serde_json::json!(top_r);
    v["notch_bot_r"] = serde_json::json!(bot_r);
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    if let Ok(s) = serde_json::to_string_pretty(&v) {
        let _ = std::fs::write(&path, s);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn load_defaults_when_missing() {
        // 用一个不存在的 HOME 验证默认值路径
        std::env::set_var("HOME", "/tmp/k9-cfg-nonexistent");
        let cfg = load();
        assert_eq!(cfg.style, EndStyle::Notch);
        assert!((cfg.widen_factor - DEFAULT_WIDEN_FACTOR).abs() < 0.001);
        assert!((cfg.top_r - DEFAULT_TOP_R).abs() < 0.001);
        assert!((cfg.bot_r - DEFAULT_BOT_R).abs() < 0.001);
    }
}
