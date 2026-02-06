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

/// 按键映射配置
#[cfg(not(test))]
pub struct ModeConfig;

#[cfg(not(test))]
impl ModeConfig {
    /// 获取某模式下某按键的行为
    /// key_index: 0-8 对应 9 个按键
    pub fn get_action(mode: KeyboardMode, key_index: u8) -> KeyAction {
        use KeyboardMode::*;
        use crate::keycode_defs::KeyCode::*;
        use crate::keycode_defs::Consumer::*;

        match (mode, key_index) {
            // MEDIA 模式 (媒体控制)
            (Media, 0) => KeyAction::Key(MediaPlayPause),
            (Media, 1) => KeyAction::Key(MediaStop),
            (Media, 2) => KeyAction::Key(MediaNextTrack),
            (Media, 3) => KeyAction::Key(MediaPrevTrack),
            (Media, 4) => KeyAction::Key(VolumeUp),
            (Media, 5) => KeyAction::Key(VolumeDown),
            (Media, 6) => KeyAction::Key(VolumeMute),
            (Media, 7) => KeyAction::Consumer(BrightnessUp),
            (Media, 8) => KeyAction::ModeSwitch, // 切模式

            // EXCEL 模式 (表格快捷)
            (Excel, 0) => KeyAction::KeyCombo(&[LControl, Home]), // Ctrl+Home
            (Excel, 1) => KeyAction::KeyCombo(&[LControl, End]),  // Ctrl+End
            (Excel, 2) => KeyAction::KeyCombo(&[LControl, C]),    // 复制
            (Excel, 3) => KeyAction::KeyCombo(&[LControl, V]),    // 粘贴
            (Excel, 4) => KeyAction::KeyCombo(&[LControl, Z]),    // 撤销
            (Excel, 5) => KeyAction::KeyCombo(&[LControl, Y]),    // 重做
            (Excel, 6) => KeyAction::KeyCombo(&[LControl, S]),    // 保存
            (Excel, 7) => KeyAction::KeyCombo(&[LControl, F]),    // 查找
            (Excel, 8) => KeyAction::ModeSwitch,

            // CLAUDE 模式 (AI助手)
            (Claude, 0) => KeyAction::KeyCombo(&[LControl, LShift, K]), // Claude 新对话
            (Claude, 1) => KeyAction::KeyCombo(&[LControl, LShift, O]), // 打开 Claude
            (Claude, 2) => KeyAction::Text("解释这段代码"),
            (Claude, 3) => KeyAction::Text("优化这段代码"),
            (Claude, 4) => KeyAction::Text("生成测试用例"),
            (Claude, 5) => KeyAction::Text("添加注释"),
            (Claude, 6) => KeyAction::KeyCombo(&[LControl, Return]), // 发送
            (Claude, 7) => KeyAction::KeyCombo(&[LShift, Return]),   // 换行
            (Claude, 8) => KeyAction::ModeSwitch,

            _ => KeyAction::None,
        }
    }
}

// 占位类型，实际要用 RMK 的键码定义
#[cfg(not(test))]
pub enum KeyAction {
    None,
    Key(crate::keycode_defs::KeyCode),
    KeyCombo(&'static [crate::keycode_defs::KeyCode]),
    Consumer(crate::keycode_defs::Consumer),
    Text(&'static str),
    ModeSwitch,
}

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
