// mode.rs - 键盘模式管理
//
// 设计原则：
// - 纯逻辑与硬件依赖分离，便于单元测试
// - 全局状态使用条件编译

#[cfg(not(test))]
use embassy_sync::blocking_mutex::raw::ThreadModeRawMutex;
#[cfg(not(test))]
use embassy_sync::watch::Watch;

/// 键盘模式（对应 RMK Layer）
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum KeyboardMode {
    #[default]
    PadA,  // Layer 0
    PadB,  // Layer 1
    PadC,  // Layer 2
}

impl KeyboardMode {
    pub const ALL: [KeyboardMode; 3] = [
        KeyboardMode::PadA,
        KeyboardMode::PadB,
        KeyboardMode::PadC,
    ];

    pub fn name(&self) -> &'static str {
        match self {
            KeyboardMode::PadA => "Layer 0",
            KeyboardMode::PadB => "Layer 1",
            KeyboardMode::PadC => "Layer 2",
        }
    }

    pub fn layer_index(&self) -> u8 {
        match self {
            KeyboardMode::PadA => 0,
            KeyboardMode::PadB => 1,
            KeyboardMode::PadC => 2,
        }
    }

    pub fn from_layer(layer: u8) -> Self {
        match layer {
            1 => Self::PadB,
            2 => Self::PadC,
            _ => Self::PadA,
        }
    }

    pub fn next(&self) -> KeyboardMode {
        match self {
            KeyboardMode::PadA => KeyboardMode::PadB,
            KeyboardMode::PadB => KeyboardMode::PadC,
            KeyboardMode::PadC => KeyboardMode::PadA,
        }
    }

    pub fn prev(&self) -> KeyboardMode {
        match self {
            KeyboardMode::PadA => KeyboardMode::PadC,
            KeyboardMode::PadB => KeyboardMode::PadA,
            KeyboardMode::PadC => KeyboardMode::PadB,
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
        assert_eq!(mode, KeyboardMode::PadA);
    }

    #[test]
    fn test_keyboard_mode_all() {
        assert_eq!(KeyboardMode::ALL.len(), 3);
        assert_eq!(KeyboardMode::ALL[0], KeyboardMode::PadA);
        assert_eq!(KeyboardMode::ALL[1], KeyboardMode::PadB);
        assert_eq!(KeyboardMode::ALL[2], KeyboardMode::PadC);
    }

    #[test]
    fn test_keyboard_mode_name() {
        assert_eq!(KeyboardMode::PadA.name(), "Layer 0");
        assert_eq!(KeyboardMode::PadB.name(), "Layer 1");
        assert_eq!(KeyboardMode::PadC.name(), "Layer 2");
    }

    #[test]
    fn test_keyboard_mode_layer_index() {
        assert_eq!(KeyboardMode::PadA.layer_index(), 0);
        assert_eq!(KeyboardMode::PadB.layer_index(), 1);
        assert_eq!(KeyboardMode::PadC.layer_index(), 2);
    }

    #[test]
    fn test_keyboard_mode_from_layer() {
        assert_eq!(KeyboardMode::from_layer(0), KeyboardMode::PadA);
        assert_eq!(KeyboardMode::from_layer(1), KeyboardMode::PadB);
        assert_eq!(KeyboardMode::from_layer(2), KeyboardMode::PadC);
        assert_eq!(KeyboardMode::from_layer(255), KeyboardMode::PadA);
    }

    #[test]
    fn test_keyboard_mode_next() {
        assert_eq!(KeyboardMode::PadA.next(), KeyboardMode::PadB);
        assert_eq!(KeyboardMode::PadB.next(), KeyboardMode::PadC);
        assert_eq!(KeyboardMode::PadC.next(), KeyboardMode::PadA);
    }

    #[test]
    fn test_keyboard_mode_prev() {
        assert_eq!(KeyboardMode::PadA.prev(), KeyboardMode::PadC);
        assert_eq!(KeyboardMode::PadB.prev(), KeyboardMode::PadA);
        assert_eq!(KeyboardMode::PadC.prev(), KeyboardMode::PadB);
    }

    #[test]
    fn test_keyboard_mode_next_cycle() {
        let mut mode = KeyboardMode::PadA;
        for _ in 0..3 {
            mode = mode.next();
        }
        assert_eq!(mode, KeyboardMode::PadA);
    }

    #[test]
    fn test_keyboard_mode_prev_cycle() {
        let mut mode = KeyboardMode::PadA;
        for _ in 0..3 {
            mode = mode.prev();
        }
        assert_eq!(mode, KeyboardMode::PadA);
    }

    #[test]
    fn test_keyboard_mode_next_prev_inverse() {
        for mode in KeyboardMode::ALL {
            assert_eq!(mode.next().prev(), mode);
            assert_eq!(mode.prev().next(), mode);
        }
    }

    #[test]
    fn test_keyboard_mode_equality() {
        assert_eq!(KeyboardMode::PadA, KeyboardMode::PadA);
        assert_ne!(KeyboardMode::PadA, KeyboardMode::PadB);
    }

    #[test]
    fn test_keyboard_mode_clone() {
        let mode = KeyboardMode::PadB;
        let cloned = mode;
        assert_eq!(mode, cloned);
    }
}
