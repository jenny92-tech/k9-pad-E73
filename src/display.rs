// display.rs - OLED 显示管理（4格电池图标 + 充电动画）
use embassy_nrf::gpio::{Level, Output, OutputDrive};
use embassy_nrf::peripherals::P0_06;
use embassy_nrf::twim::Twim;
use embassy_nrf::Peri;
use embassy_time::{Duration, Timer};
use embedded_graphics::{
    mono_font::{ascii::FONT_6X10, ascii::FONT_5X8, MonoTextStyle, MonoTextStyleBuilder},
    pixelcolor::BinaryColor,
    prelude::*,
    text::{Baseline, Text},
    primitives::{Rectangle, PrimitiveStyle, PrimitiveStyleBuilder},
};
use ssd1306::{
    prelude::*,
    size::DisplaySize,
    I2CDisplayInterface, Ssd1306,
};
use arrayvec::ArrayString;
use core::fmt::Write;

use crate::battery::{BatteryStatus, BATTERY_STATUS};
use crate::mode::{KeyboardMode, CURRENT_MODE};

// SH1107 64x128 定义
pub struct DisplaySize64x128;

impl DisplaySize for DisplaySize64x128 {
    const WIDTH: u8 = 64;
    const HEIGHT: u8 = 128;
    const DRIVER_COLS: u8 = 128;
    const DRIVER_ROWS: u8 = 128;
    const OFFSETX: u8 = 32;
    const OFFSETY: u8 = 0;
    type Buffer = [u8; (Self::WIDTH as usize * Self::HEIGHT as usize) / 8];

    fn configure(
        &self,
        iface: &mut impl WriteOnlyDataCommand,
    ) -> Result<(), display_interface::DisplayError> {
        use ssd1306::command::Command;
        Command::ComPinConfig(false, true).send(iface)
    }
}

/// 显示任务主循环
pub async fn run_display(
    i2c: Twim<'static>,
    reset: Peri<'static, P0_06>,
) {
    // 初始化复位
    let mut reset_pin = Output::new(reset, Level::High, OutputDrive::Standard);
    reset_pin.set_low();
    Timer::after(Duration::from_millis(10)).await;
    reset_pin.set_high();
    Timer::after(Duration::from_millis(10)).await;
    drop(reset_pin);

    let interface = I2CDisplayInterface::new_custom_address(i2c, 0x3c);
    let mut display = Ssd1306::new(
        interface,
        DisplaySize64x128,
        DisplayRotation::Rotate0,
    ).into_buffered_graphics_mode();
    
    if display.init().is_err() {
        defmt::warn!("OLED init failed");
        return;
    }

    // 订阅状态更新
    let mut battery_rx = BATTERY_STATUS.receiver().unwrap();
    let mut mode_rx = CURRENT_MODE.receiver().unwrap();
    
    // 当前状态缓存
    let mut battery = BatteryStatus::default();
    let mut mode = KeyboardMode::default();
    let mut frame: u8 = 0;  // 动画帧计数
    
    // 样式
    let big_style = MonoTextStyleBuilder::new()
        .font(&FONT_6X10)
        .text_color(BinaryColor::On)
        .build();
    let small_style = MonoTextStyleBuilder::new()
        .font(&FONT_5X8)
        .text_color(BinaryColor::On)
        .build();
    let fill_style = PrimitiveStyle::with_fill(BinaryColor::On);
    let stroke_style = PrimitiveStyleBuilder::new()
        .stroke_color(BinaryColor::On)
        .stroke_width(1)
        .fill_color(BinaryColor::Off)
        .build();

    loop {
        // 检查状态更新
        if let Some(b) = battery_rx.try_get() {
            battery = b;
        }
        if let Some(m) = mode_rx.try_get() {
            mode = m;
        }

        // 清屏
        display.clear(BinaryColor::Off).ok();

        // === 顶部：模式名称 ===
        Text::with_baseline(mode.name(), Point::new(2, 2), big_style, Baseline::Top)
            .draw(&mut display).ok();
        
        // 分隔线
        Rectangle::new(Point::new(0, 14), Size::new(64, 1))
            .into_styled(fill_style)
            .draw(&mut display).ok();

        // === 4格电池图标区域 ===
        draw_battery_4bar(
            &mut display, 
            battery.percentage, 
            battery.is_charging,
            frame,
            18,  // y 位置
            &fill_style,
            &stroke_style
        );

        // === 充电时显示电压，否则显示模式提示 ===
        if battery.is_charging {
            let volt_y = 38;
            let mut buf = ArrayString::<16>::new();
            write!(&mut buf, "⚡ {:.2}V", battery.voltage_mv as f32 / 1000.0).ok();
            Text::with_baseline(buf.as_str(), Point::new(4, volt_y), small_style, Baseline::Top)
                .draw(&mut display).ok();
        }

        // === 按键提示区域 ===
        let keys_y = if battery.is_charging { 52 } else { 36 };
        
        Text::with_baseline("KEYS:", Point::new(2, keys_y), small_style, Baseline::Top)
            .draw(&mut display).ok();
        
        // 画按键功能提示
        draw_mode_keys(&mut display, mode, &small_style, keys_y + 12);

        // 刷新显示
        display.flush().ok();
        
        // 动画帧递增
        frame = frame.wrapping_add(1);
        
        // 充电动画刷新更快
        Timer::after(Duration::from_millis(if battery.is_charging { 200 } else { 500 })).await;
    }
}

/// 画4格电池图标
/// 
/// 电池结构: [ 1 ][ 2 ][ 3 ][ 4 ]⚡
///           每格 4 像素宽，间隔 1 像素
fn draw_battery_4bar<D>(
    display: &mut D,
    percentage: u8,
    is_charging: bool,
    frame: u8,
    y: i32,
    fill_style: &PrimitiveStyle<BinaryColor>,
    _stroke_style: &PrimitiveStyle<BinaryColor>,
) where D: DrawTarget<Color = BinaryColor> {
    let start_x = 4;
    let bar_w: u32 = 10;  // 每格宽度
    let bar_h: u32 = 14;  // 电池高度
    let gap: i32 = 2;     // 间隔
    
    // 确定点亮几格
    let bars_lit = match percentage {
        0..=10 => 0,   // 空
        11..=30 => 1,  // 1格
        31..=55 => 2,  // 2格
        56..=80 => 3,  // 3格
        _ => 4,         // 4格满
    };
    
    // 充电动画：闪烁效果
    let charging_anim = is_charging && ((frame / 2) % 2 == 0);
    
    // 画4格电池
    for i in 0..4 {
        let x = start_x + i * (bar_w as i32 + gap);
        
        // 判断是否点亮这一格
        let should_fill = if is_charging {
            // 充电时：已充满的格常亮，正在充的格闪烁
            if i < bars_lit {
                true  // 已充满的格常亮
            } else if i == bars_lit && bars_lit < 4 {
                // 正在充的这一格闪烁
                (frame % 4) > 1
            } else {
                false
            }
        } else {
            // 非充电：按电量显示
            i < bars_lit
        };
        
        let rect = Rectangle::new(
            Point::new(x, y),
            Size::new(bar_w, bar_h)
        );
        
        if should_fill {
            rect.into_styled(*fill_style).draw(display).ok();
        } else {
            // 空格外框
            rect.into_styled(PrimitiveStyleBuilder::new()
                .stroke_color(BinaryColor::On)
                .stroke_width(1)
                .fill_color(BinaryColor::Off)
                .build())
                .draw(display).ok();
        }
    }
    
    // 充电闪电符号
    if is_charging || charging_anim {
        draw_lightning(display, start_x + 4 * (bar_w as i32 + gap) + 4, y + 2);
    }
}

/// 画闪电符号（充电指示）
fn draw_lightning<D>(display: &mut D, x: i32, y: i32) 
where D: DrawTarget<Color = BinaryColor> {
    // 8x12 闪电
    let lightning = [
        // 上半部分
        (3, 0), (4, 0),
        (2, 1), (3, 1), (4, 1),
        (1, 2), (2, 2), (3, 2),
        (1, 3), (2, 3), (3, 3),
        (2, 4), (3, 4),
        // 中间
        (3, 5), (4, 5),
        (2, 6), (3, 6), (4, 6),
        (3, 7), (4, 7),
        // 下半部分
        (4, 8), (5, 8),
        (3, 9), (4, 9), (5, 9),
        (3, 10), (4, 10),
        (4, 11),
    ];
    
    for (dx, dy) in lightning {
        Rectangle::new(
            Point::new(x + dx, y + dy), 
            Size::new(1, 1)
        ).into_styled(PrimitiveStyle::with_fill(BinaryColor::On))
         .draw(display).ok();
    }
}

/// 根据模式画按键功能提示
fn draw_mode_keys<D>(
    display: &mut D,
    mode: KeyboardMode,
    style: &MonoTextStyle<BinaryColor>,
    mut y: i32,
) where D: DrawTarget<Color = BinaryColor> {
    use crate::mode::KeyboardMode::*;
    
    let hints: &[&str] = match mode {
        Media => &[
            "1: ▶/❚❚",
            "2: ⏹ Stop",
            "3: ⏭ Next",
            "4: ⏮ Prev",
            "5: Vol+",
            "6: Vol-",
            "9: Mode→",
        ],
        Excel => &[
            "1: Ctrl+Home",
            "2: Ctrl+End", 
            "3: Copy",
            "4: Paste",
            "5: Undo",
            "6: Save",
            "9: Mode→",
        ],
        Claude => &[
            "1: New Chat",
            "2: Open",
            "3: Explain",
            "4: Optimize",
            "5: Test",
            "6: Comment",
            "9: Mode→",
        ],
    };
    
    for hint in hints.iter().take(5) {
        Text::with_baseline(hint, Point::new(2, y), *style, Baseline::Top)
            .draw(display).ok();
        y += 10;
    }
}