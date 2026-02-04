// mode.rs - 键盘模式管理
use embassy_sync::blocking_mutex::raw::ThreadModeRawMutex;
use embassy_sync::watch::Watch;
use crate::keyboard::KeyCode::*;
use crate::keyboard::Consumer::*;

/// 键盘模式
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum KeyboardMode {
    #[default]
    Media,   // 媒体控制
    Excel,   // 表格快捷
    Claude,  // AI助手
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
}

// 当前模式全局状态
pub static CURRENT_MODE: Watch<ThreadModeRawMutex, KeyboardMode, 1> = Watch::new();

/// 按键映射配置
pub struct ModeConfig;

impl ModeConfig {
    /// 获取某模式下某按键的行为
    /// key_index: 0-8 对应 9 个按键
    pub fn get_action(mode: KeyboardMode, key_index: u8) -> KeyAction {
        use KeyboardMode::*;
        
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
            (Media, 8) => KeyAction::ModeSwitch,  // 切模式
            
            // EXCEL 模式 (表格快捷)
            (Excel, 0) => KeyAction::KeyCombo(&[LControl, Home]),     // Ctrl+Home
            (Excel, 1) => KeyAction::KeyCombo(&[LControl, End]),      // Ctrl+End
            (Excel, 2) => KeyAction::KeyCombo(&[LControl, C]),        // 复制
            (Excel, 3) => KeyAction::KeyCombo(&[LControl, V]),        // 粘贴
            (Excel, 4) => KeyAction::KeyCombo(&[LControl, Z]),        // 撤销
            (Excel, 5) => KeyAction::KeyCombo(&[LControl, Y]),        // 重做
            (Excel, 6) => KeyAction::KeyCombo(&[LControl, S]),        // 保存
            (Excel, 7) => KeyAction::KeyCombo(&[LControl, F]),        // 查找
            (Excel, 8) => KeyAction::ModeSwitch,
            
            // CLAUDE 模式 (AI助手)
            (Claude, 0) => KeyAction::KeyCombo(&[LControl, LShift, K]),  // Claude 新对话
            (Claude, 1) => KeyAction::KeyCombo(&[LControl, LShift, O]),  // 打开 Claude
            (Claude, 2) => KeyAction::Text("解释这段代码"),
            (Claude, 3) => KeyAction::Text("优化这段代码"),
            (Claude, 4) => KeyAction::Text("生成测试用例"),
            (Claude, 5) => KeyAction::Text("添加注释"),
            (Claude, 6) => KeyAction::KeyCombo(&[LControl, Return]),     // 发送
            (Claude, 7) => KeyAction::KeyCombo(&[LShift, Return]),       // 换行
            (Claude, 8) => KeyAction::ModeSwitch,
            
            _ => KeyAction::None,
        }
    }
}

// 占位类型，实际要用 RMK 的键码定义
pub enum KeyAction {
    None,
    Key(crate::KeyCode),
    KeyCombo(&'static [crate::KeyCode]),
    Consumer(crate::Consumer),
    Text(&'static str),
    ModeSwitch,
}