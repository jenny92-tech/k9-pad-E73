// INPUT:  gpui::Window、notchkit-macos、objc2-foundation（管理窗口定位所需 NSRect）
// OUTPUT: K9 兼容的 NativeHandle/TOP_BLEED 重导出，以及 prepare/notched_screen_frame 薄适配
// POS:    刘海面板的 GPUI→NotchKit macOS 边界；K9 单实例显式优先物理刘海屏，原生测量/窗口/frame/点击穿透由公共库负责
use objc2_foundation::{NSPoint, NSRect, NSSize};

pub use notchkit_macos::{PanelHandle as NativeHandle, TOP_BLEED};

/// 将 GPUI 窗口交给公共 macOS 后端准备；失败时保留纯 GPUI 降级路径。
pub fn prepare(window: &gpui::Window) -> Option<NativeHandle> {
    match notchkit_macos::prepare_with_screen_preference(
        window,
        notchkit_macos::ScreenPreference::PhysicalNotchOrWindow,
    ) {
        Ok(handle) => Some(handle),
        Err(error) => {
            log::warn!("notchkit macOS preparation failed: {error}");
            None
        }
    }
}

/// 保留 K9 管理窗口所需的 NSRect API，屏幕检测本身由公共库负责。
pub fn notched_screen_frame() -> Option<NSRect> {
    match notchkit_macos::notched_screen_frame() {
        Ok(frame) => frame.map(|frame| {
            NSRect::new(
                NSPoint::new(frame.origin.x, frame.origin.y),
                NSSize::new(frame.width, frame.height),
            )
        }),
        Err(error) => {
            log::warn!("notchkit screen measurement failed: {error}");
            None
        }
    }
}
