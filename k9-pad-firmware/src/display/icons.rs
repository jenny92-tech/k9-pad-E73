// INPUT:  embedded_graphics
// OUTPUT: draw_battery_icon(), draw_ble_icon()
// POS:    状态栏图标绘制（电池 + BLE 连接状态）

use embedded_graphics::{
    pixelcolor::BinaryColor,
    prelude::*,
    primitives::{Line, PrimitiveStyle, Rectangle},
};

/// 绘制电池图标 (11x6 像素)
pub fn draw_battery_icon<D>(display: &mut D, x: i32, y: i32, percent: u8)
where
    D: DrawTarget<Color = BinaryColor>,
{
    // 电池外框 9x6
    let _ = Rectangle::new(Point::new(x, y), Size::new(9, 6))
        .into_styled(PrimitiveStyle::with_stroke(BinaryColor::On, 1))
        .draw(display);
    // 电池头 2x4 (垂直居中于 6px 高的电池体)
    let _ = Rectangle::new(Point::new(x + 9, y + 1), Size::new(2, 4))
        .into_styled(PrimitiveStyle::with_fill(BinaryColor::On))
        .draw(display);
    // 电量填充 (最多7格宽)
    let fill = (percent as u32 * 7) / 100;
    if fill > 0 {
        let _ = Rectangle::new(Point::new(x + 1, y + 1), Size::new(fill, 4))
            .into_styled(PrimitiveStyle::with_fill(BinaryColor::On))
            .draw(display);
    }
}

/// 绘制蓝牙图标 (6x9 像素)
pub fn draw_ble_icon<D>(display: &mut D, x: i32, y: i32, connected: bool)
where
    D: DrawTarget<Color = BinaryColor>,
{
    let style = PrimitiveStyle::with_stroke(BinaryColor::On, 1);

    // 蓝牙 logo
    // 中间竖线
    let _ = Line::new(Point::new(x + 3, y), Point::new(x + 3, y + 8))
        .into_styled(style)
        .draw(display);
    // 右上箭头
    let _ = Line::new(Point::new(x + 3, y), Point::new(x + 5, y + 2))
        .into_styled(style)
        .draw(display);
    // 右下箭头
    let _ = Line::new(Point::new(x + 5, y + 6), Point::new(x + 3, y + 8))
        .into_styled(style)
        .draw(display);
    // 左上交叉线
    let _ = Line::new(Point::new(x, y + 2), Point::new(x + 5, y + 6))
        .into_styled(style)
        .draw(display);
    // 左下交叉线
    let _ = Line::new(Point::new(x, y + 6), Point::new(x + 5, y + 2))
        .into_styled(style)
        .draw(display);

    if connected {
        // 已连接: 旁边画小点表示信号
        let _ = Rectangle::new(Point::new(x + 7, y + 3), Size::new(1, 3))
            .into_styled(PrimitiveStyle::with_fill(BinaryColor::On))
            .draw(display);
    } else {
        // 未连接: 画小 X
        let _ = Line::new(Point::new(x + 7, y + 2), Point::new(x + 9, y + 6))
            .into_styled(style)
            .draw(display);
        let _ = Line::new(Point::new(x + 9, y + 2), Point::new(x + 7, y + 6))
            .into_styled(style)
            .draw(display);
    }
}
