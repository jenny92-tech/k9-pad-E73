// INPUT:  embedded_graphics, display::icons, display::format, data_channel::DisplaySlotData, mode
// OUTPUT: draw_keyboard_ui(), draw_data_channel_ui()
// POS:    首页 UI 渲染（键盘状态 + 数据通道布局）

use embedded_graphics::{
    mono_font::{ascii::FONT_9X15_BOLD, MonoTextStyle},
    pixelcolor::BinaryColor,
    prelude::*,
    primitives::{Line, PrimitiveStyle, Rectangle},
    text::{Alignment, Text},
};

use crate::data_channel::DisplaySlotData;
use super::format::{format_i32, format_progress};
use super::icons::{draw_battery_icon, draw_ble_icon};

/// 绘制键盘状态界面（首页）
pub fn draw_keyboard_ui<D>(display: &mut D, mode: &str, battery_percent: u8, ble_connected: bool)
where
    D: DrawTarget<Color = BinaryColor>,
{
    let _ = display.clear(BinaryColor::Off);

    // 右上角状态图标区 (蓝牙 + 电池) — 与 data_channel_ui 坐标一致
    draw_ble_icon(display, 98, 1, ble_connected);
    draw_battery_icon(display, 112, 2, battery_percent);

    // 模式样式
    let title_style = MonoTextStyle::new(&FONT_9X15_BOLD, BinaryColor::On);

    // 绘制模式 (大字居中显示)
    let _ = Text::with_alignment(mode, Point::new(64, 40), title_style, Alignment::Center)
        .draw(display);
}

/// 绘制数据通道布局（首页模式 2：浮动头部 + 内容区）
///
/// ```text
/// ┌────────────────────────────────────┐
/// │ Kpad A        BLE ● BAT 85%       │  ← 顶部状态栏
/// │ ──────────────────────────────────  │
/// │                                    │
/// │   Volume: 75%  ████████░░          │  ← 内容区：当前 slot 数据
/// │                                    │
/// └────────────────────────────────────┘
/// ```
pub fn draw_data_channel_ui<D>(
    display: &mut D,
    mode: &str,
    battery_percent: u8,
    ble_connected: bool,
    slot_data: Option<&DisplaySlotData>,
)
where
    D: DrawTarget<Color = BinaryColor>,
{
    let _ = display.clear(BinaryColor::Off);

    // 顶部状态栏
    let small_style = MonoTextStyle::new(
        &embedded_graphics::mono_font::ascii::FONT_6X10,
        BinaryColor::On,
    );

    // 左上: Pad 名称
    let _ = Text::new(mode, Point::new(2, 9), small_style).draw(display);

    // 右上: BLE + 电池
    draw_ble_icon(display, 98, 1, ble_connected);
    draw_battery_icon(display, 112, 2, battery_percent);

    // 分隔线
    let line_style = PrimitiveStyle::with_stroke(BinaryColor::On, 1);
    let _ = Line::new(Point::new(0, 13), Point::new(127, 13))
        .into_styled(line_style)
        .draw(display);

    // 内容区
    let content_style = MonoTextStyle::new(
        &embedded_graphics::mono_font::ascii::FONT_6X10,
        BinaryColor::On,
    );

    match slot_data {
        Some(DisplaySlotData::Text(text)) => {
            let _ = Text::new(text.as_str(), Point::new(4, 38), content_style).draw(display);
        }
        Some(DisplaySlotData::Numeric(value)) => {
            let mut buf = [0u8; 16];
            let s = format_i32(*value, &mut buf);
            let _ = Text::new(s, Point::new(4, 38), content_style).draw(display);
        }
        Some(DisplaySlotData::Progress(pct)) => {
            // 百分比文字
            let mut buf = [0u8; 8];
            let s = format_progress(*pct, &mut buf);
            let _ = Text::new(s, Point::new(4, 30), content_style).draw(display);

            // 进度条 (100x8 像素)
            let bar_x = 4i32;
            let bar_y = 36i32;
            let bar_w = 100u32;
            let bar_h = 8u32;

            // 外框
            let _ = Rectangle::new(
                Point::new(bar_x, bar_y),
                Size::new(bar_w, bar_h),
            )
            .into_styled(PrimitiveStyle::with_stroke(BinaryColor::On, 1))
            .draw(display);

            // 填充
            let fill_w = (*pct as u32 * (bar_w - 2)) / 100;
            if fill_w > 0 {
                let _ = Rectangle::new(
                    Point::new(bar_x + 1, bar_y + 1),
                    Size::new(fill_w, bar_h - 2),
                )
                .into_styled(PrimitiveStyle::with_fill(BinaryColor::On))
                .draw(display);
            }
        }
        Some(DisplaySlotData::Icon(_icon_id)) => {
            let _ = Text::new("[icon]", Point::new(4, 38), content_style).draw(display);
        }
        None => {
            let _ = Text::new("Waiting...", Point::new(4, 38), content_style).draw(display);
        }
    }
}
