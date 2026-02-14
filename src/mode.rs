// INPUT:  embassy_sync
// OUTPUT: KeyboardMode enum, CURRENT_MODE watch
// POS:    键盘模式管理（Layer 0/1/2/3/4），纯逻辑可测试
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
    PadD,  // Layer 3
    PadE,  // Layer 4
}

impl KeyboardMode {
    pub const ALL: [KeyboardMode; 5] = [
        KeyboardMode::PadA,
        KeyboardMode::PadB,
        KeyboardMode::PadC,
        KeyboardMode::PadD,
        KeyboardMode::PadE,
    ];

    pub fn name(&self) -> &'static str {
        match self {
            KeyboardMode::PadA => "Layer 0",
            KeyboardMode::PadB => "Layer 1",
            KeyboardMode::PadC => "Layer 2",
            KeyboardMode::PadD => "Layer 3",
            KeyboardMode::PadE => "Layer 4",
        }
    }

    pub fn layer_index(&self) -> u8 {
        match self {
            KeyboardMode::PadA => 0,
            KeyboardMode::PadB => 1,
            KeyboardMode::PadC => 2,
            KeyboardMode::PadD => 3,
            KeyboardMode::PadE => 4,
        }
    }

    pub fn from_layer(layer: u8) -> Self {
        match layer {
            1 => Self::PadB,
            2 => Self::PadC,
            3 => Self::PadD,
            4 => Self::PadE,
            _ => Self::PadA,
        }
    }

    pub fn next(&self) -> KeyboardMode {
        match self {
            KeyboardMode::PadA => KeyboardMode::PadB,
            KeyboardMode::PadB => KeyboardMode::PadC,
            KeyboardMode::PadC => KeyboardMode::PadD,
            KeyboardMode::PadD => KeyboardMode::PadE,
            KeyboardMode::PadE => KeyboardMode::PadA,
        }
    }

    pub fn prev(&self) -> KeyboardMode {
        match self {
            KeyboardMode::PadA => KeyboardMode::PadE,
            KeyboardMode::PadB => KeyboardMode::PadA,
            KeyboardMode::PadC => KeyboardMode::PadB,
            KeyboardMode::PadD => KeyboardMode::PadC,
            KeyboardMode::PadE => KeyboardMode::PadD,
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
        assert_eq!(KeyboardMode::ALL.len(), 5);
        assert_eq!(KeyboardMode::ALL[0], KeyboardMode::PadA);
        assert_eq!(KeyboardMode::ALL[1], KeyboardMode::PadB);
        assert_eq!(KeyboardMode::ALL[2], KeyboardMode::PadC);
        assert_eq!(KeyboardMode::ALL[3], KeyboardMode::PadD);
        assert_eq!(KeyboardMode::ALL[4], KeyboardMode::PadE);
    }

    #[test]
    fn test_keyboard_mode_name() {
        assert_eq!(KeyboardMode::PadA.name(), "Layer 0");
        assert_eq!(KeyboardMode::PadB.name(), "Layer 1");
        assert_eq!(KeyboardMode::PadC.name(), "Layer 2");
        assert_eq!(KeyboardMode::PadD.name(), "Layer 3");
        assert_eq!(KeyboardMode::PadE.name(), "Layer 4");
    }

    #[test]
    fn test_keyboard_mode_layer_index() {
        assert_eq!(KeyboardMode::PadA.layer_index(), 0);
        assert_eq!(KeyboardMode::PadB.layer_index(), 1);
        assert_eq!(KeyboardMode::PadC.layer_index(), 2);
        assert_eq!(KeyboardMode::PadD.layer_index(), 3);
        assert_eq!(KeyboardMode::PadE.layer_index(), 4);
    }

    #[test]
    fn test_keyboard_mode_from_layer() {
        assert_eq!(KeyboardMode::from_layer(0), KeyboardMode::PadA);
        assert_eq!(KeyboardMode::from_layer(1), KeyboardMode::PadB);
        assert_eq!(KeyboardMode::from_layer(2), KeyboardMode::PadC);
        assert_eq!(KeyboardMode::from_layer(3), KeyboardMode::PadD);
        assert_eq!(KeyboardMode::from_layer(4), KeyboardMode::PadE);
        assert_eq!(KeyboardMode::from_layer(255), KeyboardMode::PadA);
    }

    #[test]
    fn test_keyboard_mode_next() {
        assert_eq!(KeyboardMode::PadA.next(), KeyboardMode::PadB);
        assert_eq!(KeyboardMode::PadB.next(), KeyboardMode::PadC);
        assert_eq!(KeyboardMode::PadC.next(), KeyboardMode::PadD);
        assert_eq!(KeyboardMode::PadD.next(), KeyboardMode::PadE);
        assert_eq!(KeyboardMode::PadE.next(), KeyboardMode::PadA);
    }

    #[test]
    fn test_keyboard_mode_prev() {
        assert_eq!(KeyboardMode::PadA.prev(), KeyboardMode::PadE);
        assert_eq!(KeyboardMode::PadB.prev(), KeyboardMode::PadA);
        assert_eq!(KeyboardMode::PadC.prev(), KeyboardMode::PadB);
        assert_eq!(KeyboardMode::PadD.prev(), KeyboardMode::PadC);
        assert_eq!(KeyboardMode::PadE.prev(), KeyboardMode::PadD);
    }

    #[test]
    fn test_keyboard_mode_next_cycle() {
        let mut mode = KeyboardMode::PadA;
        for _ in 0..5 {
            mode = mode.next();
        }
        assert_eq!(mode, KeyboardMode::PadA);
    }

    #[test]
    fn test_keyboard_mode_prev_cycle() {
        let mut mode = KeyboardMode::PadA;
        for _ in 0..5 {
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
