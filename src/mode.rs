// mode.rs - 键盘模式管理
//
// 设计原则：
// - 纯逻辑与硬件依赖分离，便于单元测试
// - 全局状态使用条件编译

#[cfg(not(test))]
use embassy_sync::blocking_mutex::raw::ThreadModeRawMutex;
#[cfg(not(test))]
use embassy_sync::watch::Watch;

/// 键盘模式
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum KeyboardMode {
    #[default]
    Media,  // 媒体控制
    Excel,  // 表格快捷
    Claude, // AI助手
}

impl KeyboardMode {
    pub const ALL: [KeyboardMode; 3] = [
        KeyboardMode::Media,
        KeyboardMode::Excel,
        KeyboardMode::Claude,
    ];

    pub fn name(&self) -> &'static str {
        match self {
            KeyboardMode::Media => "MEDIA",
            KeyboardMode::Excel => "EXCEL",
            KeyboardMode::Claude => "CLAUDE",
        }
    }

    pub fn next(&self) -> KeyboardMode {
        match self {
            KeyboardMode::Media => KeyboardMode::Excel,
            KeyboardMode::Excel => KeyboardMode::Claude,
            KeyboardMode::Claude => KeyboardMode::Media,
        }
    }

    pub fn prev(&self) -> KeyboardMode {
        match self {
            KeyboardMode::Media => KeyboardMode::Claude,
            KeyboardMode::Excel => KeyboardMode::Media,
            KeyboardMode::Claude => KeyboardMode::Excel,
        }
    }
}

// 当前模式全局状态（仅非测试环境）
#[cfg(not(test))]
pub static CURRENT_MODE: Watch<ThreadModeRawMutex, KeyboardMode, 1> = Watch::new();

// ============== 单元测试 ==============

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_keyboard_mode_default() {
        let mode = KeyboardMode::default();
        assert_eq!(mode, KeyboardMode::Media);
    }

    #[test]
    fn test_keyboard_mode_all() {
        assert_eq!(KeyboardMode::ALL.len(), 3);
        assert_eq!(KeyboardMode::ALL[0], KeyboardMode::Media);
        assert_eq!(KeyboardMode::ALL[1], KeyboardMode::Excel);
        assert_eq!(KeyboardMode::ALL[2], KeyboardMode::Claude);
    }

    #[test]
    fn test_keyboard_mode_name() {
        assert_eq!(KeyboardMode::Media.name(), "MEDIA");
        assert_eq!(KeyboardMode::Excel.name(), "EXCEL");
        assert_eq!(KeyboardMode::Claude.name(), "CLAUDE");
    }

    #[test]
    fn test_keyboard_mode_next() {
        assert_eq!(KeyboardMode::Media.next(), KeyboardMode::Excel);
        assert_eq!(KeyboardMode::Excel.next(), KeyboardMode::Claude);
        assert_eq!(KeyboardMode::Claude.next(), KeyboardMode::Media);
    }

    #[test]
    fn test_keyboard_mode_prev() {
        assert_eq!(KeyboardMode::Media.prev(), KeyboardMode::Claude);
        assert_eq!(KeyboardMode::Excel.prev(), KeyboardMode::Media);
        assert_eq!(KeyboardMode::Claude.prev(), KeyboardMode::Excel);
    }

    #[test]
    fn test_keyboard_mode_next_cycle() {
        // 验证 next() 循环
        let mut mode = KeyboardMode::Media;
        for _ in 0..3 {
            mode = mode.next();
        }
        assert_eq!(mode, KeyboardMode::Media);
    }

    #[test]
    fn test_keyboard_mode_prev_cycle() {
        // 验证 prev() 循环
        let mut mode = KeyboardMode::Media;
        for _ in 0..3 {
            mode = mode.prev();
        }
        assert_eq!(mode, KeyboardMode::Media);
    }

    #[test]
    fn test_keyboard_mode_next_prev_inverse() {
        // 验证 next() 和 prev() 互为逆操作
        for mode in KeyboardMode::ALL {
            assert_eq!(mode.next().prev(), mode);
            assert_eq!(mode.prev().next(), mode);
        }
    }

    #[test]
    fn test_keyboard_mode_equality() {
        assert_eq!(KeyboardMode::Media, KeyboardMode::Media);
        assert_ne!(KeyboardMode::Media, KeyboardMode::Excel);
    }

    #[test]
    fn test_keyboard_mode_clone() {
        let mode = KeyboardMode::Excel;
        let cloned = mode;
        assert_eq!(mode, cloned);
    }
}
