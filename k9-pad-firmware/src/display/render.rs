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

/// 绘制通用组件网格（新架构：host 声明 LAYOUT_CFG/COMP_LAYOUT，设备本地渲染）
///
/// ```text
/// ┌────────────────────────────┐
/// │ Kpad A      BLE ● BAT 85%  │  ← 状态栏（show_status=true 时）
/// ├────────────────────────────┤
/// │ Vol ████████░░  Time 17:52 │  ← rows×cols 网格，每格一个组件
/// │ Subs 12345      AI 60%     │
/// └────────────────────────────┘
/// ```
pub fn draw_component_grid<D>(
    display: &mut D,
    mode: &str,
    battery_percent: u8,
    ble_connected: bool,
    cache: &crate::data_channel::CompCache,
) where
    D: DrawTarget<Color = BinaryColor>,
{
    let _ = display.clear(BinaryColor::Off);

    // 状态栏（host 通过 show_status 控制：内容多就显示、少就隐藏）
    let status_h: i32 = if cache.show_status { 13 } else { 0 };
    if cache.show_status {
        let small_style = MonoTextStyle::new(
            &embedded_graphics::mono_font::ascii::FONT_6X10,
            BinaryColor::On,
        );
        let _ = Text::new(mode, Point::new(2, 9), small_style).draw(display);
        draw_ble_icon(display, 98, 1, ble_connected);
        draw_battery_icon(display, 112, 2, battery_percent);
        let line_style = PrimitiveStyle::with_stroke(BinaryColor::On, 1);
        let _ = Line::new(Point::new(0, 13), Point::new(127, 13))
            .into_styled(line_style)
            .draw(display);
    }

    // 网格：每格一个组件（cell 自适应屏幕）
    let rows = (cache.rows.max(1) as i32).min(4);
    let cols = (cache.cols.max(1) as i32).min(4);
    let cell_w = (128 / cols).max(16);
    let cell_h = ((64 - status_h) / rows).max(10);

    for comp in cache.comps.iter().flatten() {
        let x = comp.col as i32 * cell_w;
        let y = status_h + comp.row as i32 * cell_h;
        draw_comp(display, comp, x, y, cell_w, cell_h);
    }
}

/// 在格子里绘制单个组件（紧凑布局：一行 label+值；Progress 额外一行进度条）。
/// 格子 ~64x25（2×2）：6x10 字体，一行 10 字符。
fn draw_comp<D>(
    display: &mut D,
    comp: &crate::data_channel::Comp,
    x: i32,
    y: i32,
    cell_w: i32,
    cell_h: i32,
) where
    D: DrawTarget<Color = BinaryColor>,
{
    let small = MonoTextStyle::new(
        &embedded_graphics::mono_font::ascii::FONT_6X10,
        BinaryColor::On,
    );

    // 第一行：label + 值（一行放不下就截断 label）
    let mut line = [0u8; 24];
    let mut n = 0;
    let mut push = |s: &str| {
        for b in s.as_bytes() {
            if n < line.len() {
                line[n] = *b;
                n += 1;
            }
        }
    };
    push(comp.label.as_str());

    match &comp.value {
        crate::data_channel::CompValue::Text(t) => {
            // 第一行 label，第二行值文本
            let _ = Text::new(
                truncate(comp.label.as_str(), (cell_w / 6).max(1) as usize),
                Point::new(x + 2, y + 8),
                small,
            )
            .draw(display);
            let _ = Text::new(
                truncate(t.as_str(), (cell_w / 6).max(1) as usize),
                Point::new(x + 2, y + cell_h - 4),
                small,
            )
            .draw(display);
            return;
        }
        crate::data_channel::CompValue::Numeric(v) => {
            push(" ");
            let mut buf = [0u8; 16];
            push(crate::display::format::format_i32(*v, &mut buf));
        }
        crate::data_channel::CompValue::Progress(pct) => {
            push(" ");
            let mut buf = [0u8; 8];
            push(crate::display::format::format_progress(*pct, &mut buf));
            // 第一行 + 进度条
            let _ = Text::new(
                core::str::from_utf8(&line[..n]).unwrap_or(""),
                Point::new(x + 2, y + 8),
                small,
            )
            .draw(display);
            draw_progress_bar(display, x + 2, y + cell_h - 9, cell_w - 4, *pct);
            return;
        }
        crate::data_channel::CompValue::Percentage(pct) => {
            push(" ");
            let mut buf = [0u8; 8];
            push(crate::display::format::format_progress(*pct, &mut buf));
        }
        crate::data_channel::CompValue::Checkbox(on) => {
            push(if *on { " [x]" } else { " [ ]" });
        }
        crate::data_channel::CompValue::Icon(_) => {
            push(" [icon]");
        }
    }

    let _ = Text::new(
        core::str::from_utf8(&line[..n]).unwrap_or(""),
        Point::new(x + 2, y + 8),
        small,
    )
    .draw(display);
}

/// 按字符宽度截断文本（FONT_6X10：每字符 6px）
fn truncate<'a>(s: &'a str, max_chars: usize) -> &'a str {
    let mut end = 0;
    for (i, c) in s.char_indices() {
        if i >= max_chars * 6 {
            break;
        }
        end = i + c.len_utf8();
    }
    &s[..end]
}

/// 画进度条（外框 + 填充），尺寸自适应格子
fn draw_progress_bar<D>(display: &mut D, x: i32, y: i32, w: i32, pct: u8)
where
    D: DrawTarget<Color = BinaryColor>,
{
    let bar_w = w.max(8) as u32;
    let bar_h = 8u32;
    let _ = Rectangle::new(Point::new(x, y), Size::new(bar_w, bar_h))
        .into_styled(PrimitiveStyle::with_stroke(BinaryColor::On, 1))
        .draw(display);
    let fill_w = (pct as u32 * (bar_w - 2)) / 100;
    if fill_w > 0 {
        let _ = Rectangle::new(
            Point::new(x + 1, y + 1),
            Size::new(fill_w, bar_h - 2),
        )
        .into_styled(PrimitiveStyle::with_fill(BinaryColor::On))
        .draw(display);
    }
}

/// 绘制弹窗选择器（AI 授权/多选）：标题 + 选项列表，滚轮切换高亮。
///
/// 布局（128x64，4 个选项都放得下）：
/// ```text
/// ┌────────────────────────────┐
/// │ Permission Request         │  ← 标题（6x10）
/// │ ──────────────────────────  │
/// │ ▓ Allow                    │  ← 选中项（反白）
/// │   Deny                     │
/// │   Ask each time            │
/// └────────────────────────────┘
/// ```
pub fn draw_dialog<D>(
    display: &mut D,
    title: &str,
    options: &[Option<heapless::String<56>>; 4],
    selection: u8,
) where
    D: DrawTarget<Color = BinaryColor>,
{
    let _ = display.clear(BinaryColor::Off);

    let small = MonoTextStyle::new(
        &embedded_graphics::mono_font::ascii::FONT_6X10,
        BinaryColor::On,
    );

    // 标题（截断）
    let _ = Text::new(truncate(title, 20), Point::new(2, 8), small).draw(display);

    // 分隔线
    let line_style = PrimitiveStyle::with_stroke(BinaryColor::On, 1);
    let _ = Line::new(Point::new(0, 14), Point::new(127, 14))
        .into_styled(line_style)
        .draw(display);

    // 选项列表：每行 10px，选中项反白（黑底白字）
    let mut y: i32 = 22;
    for (i, opt) in options.iter().enumerate() {
        if let Some(label) = opt {
            if y > 54 {
                break; // 屏高 64：4 个选项最多到 y=52
            }
            let selected = i as u8 == selection;
            if selected {
                let _ = Rectangle::new(Point::new(0, y), Size::new(128, 10))
                    .into_styled(PrimitiveStyle::with_fill(BinaryColor::On))
                    .draw(display);
                // 反白文字：黑字
                let inverted = MonoTextStyle::new(
                    &embedded_graphics::mono_font::ascii::FONT_6X10,
                    BinaryColor::Off,
                );
                let _ = Text::new(
                    truncate(label.as_str(), 20),
                    Point::new(2, y),
                    inverted,
                )
                .draw(display);
            } else {
                let _ = Text::new(truncate(label.as_str(), 20), Point::new(2, y), small)
                    .draw(display);
            }
            y += 10;
        }
    }
}
