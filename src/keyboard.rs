// keyboard.rs - 键码定义（简化版，实际需要对接 RMK 的键码）
// 这些是占位定义，实际编译时需要替换为 RMK 的真实键码

#[derive(Clone, Copy, Debug)]
pub enum KeyCode {
    // 媒体键
    MediaPlayPause,
    MediaStop,
    MediaNextTrack,
    MediaPrevTrack,
    VolumeUp,
    VolumeDown,
    VolumeMute,
    
    // 修饰键
    LControl,
    LShift,
    LAlt,
    LGui,
    
    // 常用键
    Home,
    End,
    Return,
    Space,
    Tab,
    Escape,
    
    // 字母
    A, B, C, D, E, F, G, H, I, J, K, L, M,
    N, O, P, Q, R, S, T, U, V, W, X, Y, Z,
    
    // 数字
    N1, N2, N3, N4, N5, N6, N7, N8, N9, N0,
}

#[derive(Clone, Copy, Debug)]
pub enum Consumer {
    BrightnessUp,
    BrightnessDown,
    Eject,
}