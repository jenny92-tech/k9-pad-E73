// INPUT:  embassy_sync
// OUTPUT: KeyboardMode struct, NUM_LAYERS const, CURRENT_MODE watch
// POS:    键盘模式管理（Layer 0..NUM_LAYERS-1），NUM_LAYERS 为唯一真相源
// mode.rs - 键盘模式管理
//
// 设计原则：
// - NUM_LAYERS 是 Layer 数量的唯一真相源 (Rust 侧)
// - 纯逻辑与硬件依赖分离，便于单元测试
// - 全局状态使用条件编译

#[cfg(not(test))]
use embassy_sync::blocking_mutex::raw::ThreadModeRawMutex;
#[cfg(not(test))]
use embassy_sync::watch::Watch;

/// Layer 数量 — 唯一真相源 (Rust 侧)
/// SYNC: 必须与 WouoUI_k9pad.c 中的 NUM_LAYERS 保持一致
pub const NUM_LAYERS: u8 = 5;

/// 键盘模式（对应 RMK Layer），内部存储 layer index
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct KeyboardMode(u8);

impl Default for KeyboardMode {
    fn default() -> Self {
        Self(0)
    }
}

impl KeyboardMode {
    pub const fn new(layer: u8) -> Self {
        Self(if layer < NUM_LAYERS { layer } else { 0 })
    }

    pub fn layer_index(&self) -> u8 {
        self.0
    }

    pub fn from_layer(layer: u8) -> Self {
        Self::new(layer)
    }

    pub fn name(&self) -> &'static str {
        // 预分配 10 个名称，运行时只用前 NUM_LAYERS 个
        const NAMES: [&str; 10] = [
            "Layer 0", "Layer 1", "Layer 2", "Layer 3", "Layer 4",
            "Layer 5", "Layer 6", "Layer 7", "Layer 8", "Layer 9",
        ];
        NAMES[self.0 as usize]
    }

    pub fn next(&self) -> KeyboardMode {
        Self((self.0 + 1) % NUM_LAYERS)
    }

    pub fn prev(&self) -> KeyboardMode {
        Self((self.0 + NUM_LAYERS - 1) % NUM_LAYERS)
    }

    pub fn all() -> &'static [KeyboardMode] {
        const ALL: [KeyboardMode; 10] = [
            KeyboardMode(0), KeyboardMode(1), KeyboardMode(2),
            KeyboardMode(3), KeyboardMode(4), KeyboardMode(5),
            KeyboardMode(6), KeyboardMode(7), KeyboardMode(8),
            KeyboardMode(9),
        ];
        &ALL[..NUM_LAYERS as usize]
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
        assert_eq!(mode, KeyboardMode::new(0));
        assert_eq!(mode.layer_index(), 0);
    }

    #[test]
    fn test_keyboard_mode_all() {
        let all = KeyboardMode::all();
        assert_eq!(all.len(), NUM_LAYERS as usize);
        for (i, mode) in all.iter().enumerate() {
            assert_eq!(mode.layer_index(), i as u8);
        }
    }

    #[test]
    fn test_keyboard_mode_name() {
        for i in 0..NUM_LAYERS {
            let mode = KeyboardMode::new(i);
            let expected = KeyboardMode::all()[i as usize].name();
            assert_eq!(mode.name(), expected);
            assert!(mode.name().starts_with("Layer "));
        }
    }

    #[test]
    fn test_keyboard_mode_layer_index() {
        for i in 0..NUM_LAYERS {
            assert_eq!(KeyboardMode::new(i).layer_index(), i);
        }
    }

    #[test]
    fn test_keyboard_mode_from_layer() {
        for i in 0..NUM_LAYERS {
            assert_eq!(KeyboardMode::from_layer(i), KeyboardMode::new(i));
        }
        // Out-of-range wraps to 0
        assert_eq!(KeyboardMode::from_layer(NUM_LAYERS), KeyboardMode::new(0));
        assert_eq!(KeyboardMode::from_layer(255), KeyboardMode::new(0));
    }

    #[test]
    fn test_keyboard_mode_next() {
        for i in 0..NUM_LAYERS {
            let mode = KeyboardMode::new(i);
            let expected = (i + 1) % NUM_LAYERS;
            assert_eq!(mode.next(), KeyboardMode::new(expected));
        }
    }

    #[test]
    fn test_keyboard_mode_prev() {
        for i in 0..NUM_LAYERS {
            let mode = KeyboardMode::new(i);
            let expected = (i + NUM_LAYERS - 1) % NUM_LAYERS;
            assert_eq!(mode.prev(), KeyboardMode::new(expected));
        }
    }

    #[test]
    fn test_keyboard_mode_next_cycle() {
        let mut mode = KeyboardMode::default();
        for _ in 0..NUM_LAYERS {
            mode = mode.next();
        }
        assert_eq!(mode, KeyboardMode::default());
    }

    #[test]
    fn test_keyboard_mode_prev_cycle() {
        let mut mode = KeyboardMode::default();
        for _ in 0..NUM_LAYERS {
            mode = mode.prev();
        }
        assert_eq!(mode, KeyboardMode::default());
    }

    #[test]
    fn test_keyboard_mode_next_prev_inverse() {
        for mode in KeyboardMode::all() {
            assert_eq!(mode.next().prev(), *mode);
            assert_eq!(mode.prev().next(), *mode);
        }
    }

    #[test]
    fn test_keyboard_mode_equality() {
        assert_eq!(KeyboardMode::new(0), KeyboardMode::new(0));
        if NUM_LAYERS > 1 {
            assert_ne!(KeyboardMode::new(0), KeyboardMode::new(1));
        }
    }

    #[test]
    fn test_keyboard_mode_clone() {
        let mode = KeyboardMode::new(1 % NUM_LAYERS);
        let cloned = mode;
        assert_eq!(mode, cloned);
    }

    #[test]
    fn test_keyboard_mode_new_out_of_range() {
        // Out-of-range layer should wrap to 0
        assert_eq!(KeyboardMode::new(NUM_LAYERS), KeyboardMode::new(0));
        assert_eq!(KeyboardMode::new(NUM_LAYERS + 5), KeyboardMode::new(0));
    }
}
