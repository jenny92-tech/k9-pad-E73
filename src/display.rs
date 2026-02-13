// display.rs - SH1107 OLED 显示管理（横屏 128x64 布局）
//
// 集成 WouoUI 菜单系统，支持：
// - 首页（键盘状态显示）
// - WouoUI 动画菜单（横向磁贴滚动）
// - 动态帧率（菜单 30 FPS，首页 1 FPS）

use embassy_nrf::gpio::{Level, Output, OutputDrive};
use embassy_nrf::peripherals::P0_06;
use embassy_nrf::twim::Twim;
use embassy_nrf::Peri;
use embassy_time::{Duration, Instant, Timer};
use embedded_graphics::{
    mono_font::{ascii::FONT_9X15_BOLD, MonoTextStyle},
    pixelcolor::BinaryColor,
    prelude::*,
    primitives::{Line, PrimitiveStyle, Rectangle},
    text::{Alignment, Text},
};
use embedded_hal_async::i2c::I2c;

use crate::data_channel::{DisplayDataCache, DisplaySlotData, DISPLAY_DATA};
use crate::menu::{MenuInput, MENU_INPUT, MENU_STATE, MenuState, PageId};
use crate::mode::CURRENT_MODE;
use crate::battery::BATTERY_STATUS;
use crate::wououi::{WouoUI, WououiInput, SCREEN_WIDTH, SCREEN_HEIGHT};
use rmk::ble::BleState;
use rmk::event::{BleStateChangeEvent, SubscribableEvent, EventSubscriber};

// SH1107 I2C 地址
const SH1107_ADDR: u8 = 0x3C;

// 显示尺寸
const DISPLAY_WIDTH: u32 = 128;
const DISPLAY_HEIGHT: u32 = 64;

/// SH1107 显示驱动 (横屏 128x64)
pub struct Sh1107<I2C> {
    i2c: I2C,
    buffer: [u8; 1024],      // 当前帧缓冲区
    prev_buffer: [u8; 1024],  // 上一帧缓冲区，用于脏页追踪
}

impl<I2C: I2c> Sh1107<I2C> {
    pub fn new(i2c: I2C) -> Self {
        Self {
            i2c,
            buffer: [0u8; 1024],
            prev_buffer: [0xFFu8; 1024], // 初始化为不同值，确保首帧全刷
        }
    }

    /// 发送命令
    pub async fn send_command(&mut self, cmd: u8) -> Result<(), I2C::Error> {
        self.i2c.write(SH1107_ADDR, &[0x00, cmd]).await
    }

    /// 初始化 SH1107
    pub async fn init(&mut self) -> Result<(), I2C::Error> {
        defmt::info!("SH1107 init...");

        let init_cmds: &[u8] = &[
            0xAE,       // Display OFF
            0x00,       // Set low column address = 0
            0x10,       // Set high column address = 0
            0xDC, 0x00, // Set display start line = 0
            0x81, 0x80, // Set contrast = 128
            0xA0,       // Set segment re-map
            0xC8,       // Set COM scan direction (remapped)
            0xA6,       // Set normal display
            0xA8, 0x7F, // Set multiplex ratio = 128
            0xD3, 0x60, // Set display offset = 96
            0xD5, 0xF0, // Set clock divide ratio (高刷新率)
            0xD9, 0x22, // Set pre-charge period
            0xDA, 0x12, // Set COM pins configuration
            0xDB, 0x35, // Set VCOMH deselect level
            0x20, 0x00, // Set horizontal addressing mode
            0xA4,       // Entire display ON (resume from RAM)
            0xA6,       // Normal display
        ];

        for cmd in init_cmds.iter() {
            self.send_command(*cmd).await?;
        }

        defmt::info!("SH1107 init done");
        Ok(())
    }

    /// 发送数据
    async fn send_data(&mut self, data: &[u8]) -> Result<(), I2C::Error> {
        const CHUNK_SIZE: usize = 64;
        let mut buf = [0u8; CHUNK_SIZE + 1];
        buf[0] = 0x40;

        for chunk in data.chunks(CHUNK_SIZE) {
            buf[1..1 + chunk.len()].copy_from_slice(chunk);
            self.i2c.write(SH1107_ADDR, &buf[..1 + chunk.len()]).await?;
        }
        Ok(())
    }

    /// 设置对比度 (亮度)
    /// value: 0-255, 对应 SH1107 contrast 寄存器
    pub async fn set_contrast(&mut self, value: u8) -> Result<(), I2C::Error> {
        self.send_command(0x81).await?;
        self.send_command(value).await
    }

    /// 清除缓冲区
    #[allow(dead_code)]
    pub fn clear(&mut self) {
        self.buffer.fill(0);
    }

    /// 刷新显示 - 脏页追踪 + 合并命令
    pub async fn flush(&mut self) -> Result<(), I2C::Error> {
        for page in 0..16u8 {
            let start = (page as usize) * 64;
            let end = start + 64;

            // 脏页追踪：跳过没变化的页
            if self.buffer[start..end] == self.prev_buffer[start..end] {
                continue;
            }

            // 合并 3 条命令到 1 次 I2C 事务
            let cmds = [0x00, 0xB0 | page, 0x00, 0x00, 0x00, 0x10];
            self.i2c.write(SH1107_ADDR, &cmds).await?;

            // 发送页数据
            let mut chunk = [0u8; 64];
            chunk.copy_from_slice(&self.buffer[start..end]);
            self.send_data(&chunk).await?;
        }

        // 保存当前帧用于下一帧比较
        self.prev_buffer.copy_from_slice(&self.buffer);
        Ok(())
    }
}

// 单独的 impl 块，不需要 I2c trait bound
impl<I2C> Sh1107<I2C> {
    /// 设置像素 - 横屏坐标映射
    fn set_pixel(&mut self, x: i32, y: i32, on: bool) {
        if x < 0 || x >= DISPLAY_WIDTH as i32 || y < 0 || y >= DISPLAY_HEIGHT as i32 {
            return;
        }

        // 横屏映射：(x, y) -> SH1107 内部坐标
        // SH1107 是 64 列 x 128 行，我们旋转 90 度使用
        let col = y as usize; // 0-63
        let row = x as usize; // 0-127
        let page = row / 8;
        let bit = row % 8;
        let idx = page * 64 + col;

        if idx < self.buffer.len() {
            if on {
                self.buffer[idx] |= 1 << bit;
            } else {
                self.buffer[idx] &= !(1 << bit);
            }
        }
    }
}

impl<I2C> DrawTarget for Sh1107<I2C> {
    type Color = BinaryColor;
    type Error = core::convert::Infallible;

    fn draw_iter<I>(&mut self, pixels: I) -> Result<(), Self::Error>
    where
        I: IntoIterator<Item = Pixel<Self::Color>>,
    {
        for Pixel(coord, color) in pixels {
            self.set_pixel(coord.x, coord.y, color.is_on());
        }
        Ok(())
    }
}

impl<I2C> OriginDimensions for Sh1107<I2C> {
    fn size(&self) -> Size {
        Size::new(DISPLAY_WIDTH, DISPLAY_HEIGHT)
    }
}

// ============== Battery Hardware (Raw Register Access) ==============
//
// SAADC 和 GPIO 通过寄存器直接访问，因为 display task 是 RMK 宏唯一
// spawn 的自定义 async 函数，无法传入额外外设。
// （与 OLED 电源开关 P0.05 使用同样的模式）

/// 配置 P0.07 (CHRG_DET) 为输入 + 上拉。
/// TP4054 CHRG# 是开漏输出：低电平 = 正在充电。
///
/// SAFETY: 访问 nRF52840 GPIO PIN_CNF[7] 寄存器。P0.07 未被 keyboard.toml
/// 中任何功能使用（battery 配置已注释掉），不存在竞争。
unsafe fn init_charge_detect_pin() {
    const P0_BASE: u32 = 0x5000_0000;
    const PIN_CNF_OFFSET: u32 = 0x700;
    // DIR=Input(0), INPUT=Connect(0), PULL=Pullup(3<<2)
    let addr = (P0_BASE + PIN_CNF_OFFSET + 7 * 4) as *mut u32;
    core::ptr::write_volatile(addr, 0x0000_000C);
}

/// 读取 P0.07 充电状态。返回 true = 正在充电（引脚低电平）。
///
/// SAFETY: 读取 nRF52840 GPIO IN 寄存器，只读操作无副作用。
unsafe fn read_charge_pin() -> bool {
    const P0_BASE: u32 = 0x5000_0000;
    const IN_OFFSET: u32 = 0x510;
    let state = core::ptr::read_volatile((P0_BASE + IN_OFFSET) as *const u32);
    (state & (1 << 7)) == 0 // Active low
}

/// 阻塞式 SAADC 单次读取 AIN6 (P0.30 = POWER_PIN)。
/// 返回 12-bit 原始值。每次读取前启用、读完后禁用 SAADC。
///
/// SAFETY: 访问 nRF52840 SAADC 寄存器。keyboard.toml 中 battery_adc_pin
/// 已注释掉，RMK 不会使用 SAADC，不存在竞争。
/// 阻塞等待时间约几十微秒，不影响 display loop。
unsafe fn read_battery_adc_raw() -> i16 {
    const SAADC: u32 = 0x4000_7000;

    // 启用 SAADC
    core::ptr::write_volatile((SAADC + 0x500) as *mut u32, 1); // ENABLE

    // Channel 0: AIN6 (P0.30), single-ended
    core::ptr::write_volatile((SAADC + 0x510) as *mut u32, 7); // CH[0].PSELP = AIN6
    core::ptr::write_volatile((SAADC + 0x514) as *mut u32, 0); // CH[0].PSELN = NC

    // CONFIG: Gain=1/6, Ref=Internal(0.6V), Tacq=40us, Mode=SE
    core::ptr::write_volatile(
        (SAADC + 0x518) as *mut u32,
        (2 << 8) | (0 << 12) | (5 << 16) | (0 << 20),
    );

    // Resolution 12-bit
    core::ptr::write_volatile((SAADC + 0x5F0) as *mut u32, 2);

    // 结果缓冲区（DMA 需要 static 地址）
    static mut ADC_BUF: i16 = 0;
    core::ptr::write_volatile(
        (SAADC + 0x62C) as *mut u32,
        core::ptr::addr_of_mut!(ADC_BUF) as u32,
    ); // RESULT.PTR
    core::ptr::write_volatile((SAADC + 0x630) as *mut u32, 1); // RESULT.MAXCNT

    // 清除事件
    core::ptr::write_volatile((SAADC + 0x100) as *mut u32, 0); // EVENTS_STARTED
    core::ptr::write_volatile((SAADC + 0x104) as *mut u32, 0); // EVENTS_END
    core::ptr::write_volatile((SAADC + 0x114) as *mut u32, 0); // EVENTS_STOPPED

    // Start → wait STARTED
    core::ptr::write_volatile((SAADC + 0x000) as *mut u32, 1); // TASKS_START
    while core::ptr::read_volatile((SAADC + 0x100) as *const u32) == 0 {}

    // Sample → wait END
    core::ptr::write_volatile((SAADC + 0x004) as *mut u32, 1); // TASKS_SAMPLE
    while core::ptr::read_volatile((SAADC + 0x104) as *const u32) == 0 {}

    // Stop → wait STOPPED
    core::ptr::write_volatile((SAADC + 0x008) as *mut u32, 1); // TASKS_STOP
    while core::ptr::read_volatile((SAADC + 0x114) as *const u32) == 0 {}

    // 禁用 SAADC
    core::ptr::write_volatile((SAADC + 0x500) as *mut u32, 0);

    ADC_BUF
}

/// 读取电池电压 (mV)。
///
/// 硬件分压: R8=820kΩ (VBAT→POWER_PIN), R10=2MΩ (POWER_PIN→GND)
/// V_adc = VBAT × R10/(R8+R10) = VBAT × 2000/2820
/// → VBAT = V_adc × 2820/2000
///
/// SAADC 公式 (SE, 12-bit, Gain=1/6, Ref=0.6V):
/// raw = V_adc × 4096 / 3600
/// → V_adc = raw × 3600 / 4096
///
/// 合并: VBAT = raw × 3600 × 2820 / (4096 × 2000) = raw × 1269 / 1024
fn read_battery_voltage_mv() -> u16 {
    let raw = unsafe { read_battery_adc_raw() }.max(0) as u32;
    ((raw * 1269) / 1024) as u16
}

/// 启用 GPIO 内部上拉
// SAFETY: Accesses nRF52840 GPIO PIN_CNF registers via raw pointers.
// P0_BASE/P1_BASE are valid memory-mapped peripheral addresses per the nRF52840
// datasheet. Setting bits [3:2] to 0b11 enables the internal pull-up resistor.
// Called once during display init before I2C communication begins.
unsafe fn enable_i2c_pullups() {
    const P0_BASE: u32 = 0x5000_0000;
    const P1_BASE: u32 = 0x5000_0300;
    const PIN_CNF_OFFSET: u32 = 0x700;

    // SDA = P0.08
    let sda_cnf_addr = (P0_BASE + PIN_CNF_OFFSET + 8 * 4) as *mut u32;
    let sda_val = core::ptr::read_volatile(sda_cnf_addr);
    core::ptr::write_volatile(sda_cnf_addr, sda_val | (3 << 2));

    // SCL = P1.09
    let scl_cnf_addr = (P1_BASE + PIN_CNF_OFFSET + 9 * 4) as *mut u32;
    let scl_val = core::ptr::read_volatile(scl_cnf_addr);
    core::ptr::write_volatile(scl_cnf_addr, scl_val | (3 << 2));

    defmt::info!("I2C pullups enabled");
}

/// 绘制电池图标 (11x6 像素)
fn draw_battery_icon<D>(display: &mut D, x: i32, y: i32, percent: u8)
where
    D: DrawTarget<Color = BinaryColor>,
{
    // 电池外框 9x6
    let _ = Rectangle::new(Point::new(x, y), Size::new(9, 6))
        .into_styled(PrimitiveStyle::with_stroke(BinaryColor::On, 1))
        .draw(display);
    // 电池头 1x2
    let _ = Rectangle::new(Point::new(x + 9, y + 2), Size::new(2, 4))
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
fn draw_ble_icon<D>(display: &mut D, x: i32, y: i32, connected: bool)
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

/// 绘制键盘状态界面（首页）
fn draw_keyboard_ui<D>(display: &mut D, mode: &str, battery_percent: u8, ble_connected: bool)
where
    D: DrawTarget<Color = BinaryColor>,
{
    let _ = display.clear(BinaryColor::Off);

    // 右上角状态图标区 (蓝牙 + 电池)
    draw_ble_icon(display, 103, 2, ble_connected);
    draw_battery_icon(display, 115, 3, battery_percent);

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
fn draw_data_channel_ui<D>(
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

/// 格式化 i32 到固定缓冲区，返回字符串切片
fn format_i32(value: i32, buf: &mut [u8; 16]) -> &str {
    let mut pos = buf.len();
    let negative = value < 0;
    let mut v = if negative {
        (value as i64).unsigned_abs()
    } else {
        value as u64
    };

    if v == 0 {
        pos -= 1;
        buf[pos] = b'0';
    } else {
        while v > 0 {
            pos -= 1;
            buf[pos] = b'0' + (v % 10) as u8;
            v /= 10;
        }
    }

    if negative {
        pos -= 1;
        buf[pos] = b'-';
    }

    core::str::from_utf8(&buf[pos..]).unwrap_or("?")
}

/// 格式化进度百分比
fn format_progress(pct: u8, buf: &mut [u8; 8]) -> &str {
    let mut pos = 0;

    // 数字部分
    if pct >= 100 {
        buf[pos] = b'1';
        pos += 1;
        buf[pos] = b'0';
        pos += 1;
        buf[pos] = b'0';
        pos += 1;
    } else if pct >= 10 {
        buf[pos] = b'0' + pct / 10;
        pos += 1;
        buf[pos] = b'0' + pct % 10;
        pos += 1;
    } else {
        buf[pos] = b'0' + pct;
        pos += 1;
    }

    buf[pos] = b'%';
    pos += 1;

    core::str::from_utf8(&buf[..pos]).unwrap_or("?%")
}

/// 将菜单输入转换为 WouoUI 输入
fn menu_input_to_wououi(input: MenuInput) -> Option<WououiInput> {
    match input {
        MenuInput::ScrollUp => Some(WououiInput::Up),
        MenuInput::ScrollDown => Some(WououiInput::Down),
        MenuInput::Select => Some(WououiInput::Click),
        MenuInput::Back => Some(WououiInput::Return),
        MenuInput::EnterMenu => None, // 特殊处理
        MenuInput::ExitMenu => None,  // 特殊处理
    }
}

/// 复制 WouoUI 缓冲区到显示缓冲区
/// WouoUI 使用 SSD1306 格式，需要转换为 SH1107 格式
fn copy_wououi_buffer_to_display<I2C>(display: &mut Sh1107<I2C>, wououi_buffer: &[u8]) {
    // 清除现有缓冲区
    display.buffer.fill(0);

    // WouoUI 缓冲区格式：列优先，每字节 8 个垂直像素
    // SH1107 格式：也是列优先，但需要旋转
    // WouoUI: buffer[col + (row/8)*WIDTH] 的 bit (row%8)
    //
    // 我们的显示是 128x64，WouoUI 也是 128x64
    // 直接遍历每个像素并设置
    for y in 0..SCREEN_HEIGHT {
        for x in 0..SCREEN_WIDTH {
            // 从 WouoUI 缓冲区读取像素
            let wououi_byte_idx = x + (y / 8) * SCREEN_WIDTH;
            let wououi_bit = y % 8;

            if wououi_byte_idx < wououi_buffer.len() {
                let pixel_on = (wououi_buffer[wououi_byte_idx] & (1 << wououi_bit)) != 0;

                if pixel_on {
                    // 写入显示缓冲区（使用 SH1107 映射）
                    let col = y;  // 0-63
                    let row = x;  // 0-127
                    let page = row / 8;
                    let bit = row % 8;
                    let idx = page * 64 + col;

                    if idx < display.buffer.len() {
                        display.buffer[idx] |= 1 << bit;
                    }
                }
            }
        }
    }
}

/// 显示任务主循环
pub async fn run_display(i2c: Twim<'static>, reset: Peri<'static, P0_06>) {
    // 启用 I2C 内部上拉
    // SAFETY: enable_i2c_pullups 访问 GPIO 寄存器配置上拉电阻。
    // 此时 I2C 外设尚未初始化，不会产生寄存器访问竞争。
    unsafe {
        enable_i2c_pullups();
    }

    // 启用 OLED 电源开关 (P0.05)
    defmt::info!("Enabling OLED power (P0.05)");
    // SAFETY: Configures P0.05 as output and sets it high to enable OLED power.
    // PIN_CNF[5] = 0x03 sets direction=output, input-disconnect.
    // OUTSET bit 5 drives the pin high. These are valid nRF52840 GPIO register
    // addresses per the datasheet. No other code accesses P0.05.
    unsafe {
        const P0_BASE: u32 = 0x5000_0000;
        const PIN_CNF_OFFSET: u32 = 0x700;
        const OUTSET_OFFSET: u32 = 0x508;

        let pin_cnf_addr = (P0_BASE + PIN_CNF_OFFSET + 5 * 4) as *mut u32;
        core::ptr::write_volatile(pin_cnf_addr, 0x0000_0003);

        let outset_addr = (P0_BASE + OUTSET_OFFSET) as *mut u32;
        core::ptr::write_volatile(outset_addr, 1 << 5);
    }

    // 等待电源稳定
    Timer::after(Duration::from_millis(500)).await;

    // 硬件复位 OLED
    defmt::info!("Resetting OLED...");
    let mut reset_pin = Output::new(reset, Level::High, OutputDrive::Standard);
    Timer::after(Duration::from_millis(100)).await;
    reset_pin.set_low();
    Timer::after(Duration::from_millis(100)).await;
    reset_pin.set_high();
    Timer::after(Duration::from_millis(100)).await;

    // 创建显示驱动
    let mut display = Sh1107::new(i2c);

    // 探测设备
    defmt::info!("Probing 0x3C...");
    match display.i2c.write(SH1107_ADDR, &[0x00]).await {
        Ok(_) => defmt::info!("Device found!"),
        Err(_) => {
            defmt::error!("Device NOT found!");
            loop {
                Timer::after(Duration::from_secs(1)).await;
            }
        }
    }

    // 初始化显示
    if let Err(_) = display.init().await {
        defmt::error!("Init failed!");
        loop {
            Timer::after(Duration::from_secs(1)).await;
        }
    }

    // 打开显示
    Timer::after(Duration::from_millis(200)).await;
    display.send_command(0xAF).await.ok();
    defmt::info!("Display ON");

    // 初始化 WouoUI
    let mut wououi = WouoUI::new();
    wououi.init();

    // 菜单状态跟踪
    let mut menu_active = false;
    let mut menu_idle_ticks: u16 = 0;
    const MENU_TIMEOUT_TICKS: u16 = 120 * 30; // 30秒 @ 120Hz

    // 数据通道显示缓存 + slot 轮播
    let mut dc_cache = DisplayDataCache::new();
    let mut dc_current_slot: u8 = 0;
    let mut dc_rotation_timer = Instant::now();
    const DC_ROTATION_INTERVAL: Duration = Duration::from_secs(4);

    // 初始化充电检测引脚 (P0.07, 上拉输入)
    // SAFETY: P0.07 未被其他任务使用，见 init_charge_detect_pin 文档
    unsafe { init_charge_detect_pin() };
    defmt::info!("Battery: charge detect pin (P0.07) initialized");

    // 获取状态发送器和接收器
    let menu_state_tx = MENU_STATE.sender();
    let battery_status_tx = BATTERY_STATUS.sender();
    let mode_tx = CURRENT_MODE.sender();

    // 初始状态
    let mut current_mode = crate::mode::KeyboardMode::default();
    mode_tx.send(current_mode);
    let mut current_pad_index: u8 = 0;
    let mut current_brightness: u8 = 80;
    let mut last_contrast_write = Instant::now();
    const CONTRAST_MIN_INTERVAL: Duration = Duration::from_millis(100);
    let mut current_ble_enabled: bool = true;
    let mut current_user: u8 = 0;
    let mut battery_status = crate::battery::BatteryStatus::default();

    // BLE 连接状态：通过 RMK 事件系统订阅（替代有 bug 的 get_connection_state 轮询）
    let mut ble_sub = BleStateChangeEvent::subscriber();
    let mut ble_connected = false;

    // 首次读取电池状态（避免启动后 5 秒内显示 0%）
    {
        let voltage = read_battery_voltage_mv();
        let percentage = crate::battery::calc_percentage(voltage);
        let is_charging = unsafe { read_charge_pin() };
        battery_status = crate::battery::BatteryStatus {
            voltage_mv: voltage,
            percentage,
            is_charging,
        };
        battery_status_tx.send(battery_status);
        defmt::info!("Battery init: {}mV {}% charging={}", voltage, percentage, is_charging);
    }

    // 电池读取计时
    let mut last_battery_read = Instant::now();
    const BATTERY_READ_INTERVAL: Duration = Duration::from_secs(5);

    // 发送初始菜单状态
    let initial_state = MenuState {
        active: false,
        current_page: PageId::Home,
        selected_index: 0,
        scroll_offset: 0,
        target_scroll_offset: 0,
    };
    menu_state_tx.send(initial_state);

    // 用于计算帧间隔
    let mut last_frame = Instant::now();

    // 启动菜单控制器（并行运行）
    let mut menu_ctrl = crate::menu::MenuController::new();
    let menu_ctrl_future = menu_ctrl.run();

    // 显示主循环
    let display_future = async {
    loop {
        let now = Instant::now();
        let elapsed_ms = (now - last_frame).as_millis() as u16;
        last_frame = now;

        // 非阻塞方式处理输入事件
        while let Ok(input) = MENU_INPUT.try_receive() {
            defmt::info!("Menu input: {:?}", defmt::Debug2Format(&input));

            // 重置空闲计时
            menu_idle_ticks = 0;

            match input {
                MenuInput::EnterMenu => {
                    if !menu_active {
                        menu_active = true;
                        wououi.enter_menu();
                        defmt::info!("WouoUI: Menu activated");
                    }
                }
                MenuInput::ExitMenu => {
                    if menu_active {
                        menu_active = false;
                        wououi.exit_menu();
                        defmt::info!("WouoUI: Menu deactivated");
                    }
                }
                MenuInput::Back => {
                    // 在主页按返回键：退出菜单
                    // 在子页面按返回键：返回上一级
                    if menu_active {
                        if wououi.is_on_home_page() {
                            menu_active = false;
                            wououi.exit_menu();
                            defmt::info!("WouoUI: Back on home page -> exit menu");
                        } else {
                            wououi.send_input(WououiInput::Return);
                        }
                    }
                }
                _ => {
                    // 转换为 WouoUI 输入（ScrollUp, ScrollDown, Select）
                    if menu_active {
                        if let Some(wououi_input) = menu_input_to_wououi(input) {
                            wououi.send_input(wououi_input);
                        }
                    }
                }
            }
        }

        // 更新电池状态（每 5 秒直接读取 SAADC + 充电引脚）
        if now.duration_since(last_battery_read) >= BATTERY_READ_INTERVAL {
            last_battery_read = now;

            let voltage = read_battery_voltage_mv();
            let percentage = crate::battery::calc_percentage(voltage);
            let is_charging = unsafe { read_charge_pin() };

            battery_status = crate::battery::BatteryStatus {
                voltage_mv: voltage,
                percentage,
                is_charging,
            };

            // 广播给其他消费者（如未来的 BLE battery service）
            battery_status_tx.send(battery_status);

            defmt::info!(
                "Battery: {}mV {}% charging={}",
                voltage,
                percentage,
                is_charging
            );

        }

        // ====== BLE 状态：非阻塞消费事件 ======
        while let Some(event) = ble_sub.try_next_message_pure() {
            let new_connected = matches!(event.state, BleState::Connected);
            if new_connected != ble_connected {
                defmt::info!(
                    ">>> BLE event: {:?}, connected: {} -> {}",
                    defmt::Debug2Format(&event.state),
                    ble_connected,
                    new_connected
                );
                ble_connected = new_connected;
            }
        }

        // 渲染
        if menu_active {
            // 菜单模式：使用 WouoUI 渲染
            // 限制帧间隔在合理范围，防止从低帧率(首页1FPS)切换时
            // 过大的 elapsed_ms 导致动画计算异常
            let clamped_elapsed = elapsed_ms.clamp(1, 50);
            let screen_updated = wououi.tick(clamped_elapsed);

            if screen_updated {
                if let Some(buffer) = wououi.get_buffer() {
                    copy_wououi_buffer_to_display(&mut display, buffer);
                }
            }

            // C 回调请求退出菜单（如 Pad 选择后）
            if wououi.take_exit_request() {
                menu_active = false;
                wououi.exit_menu();
                defmt::info!("WouoUI: Exit requested by callback");
            }

            // 检测 Pad 选择变化，切换 RMK Layer
            let selected_pad = wououi.get_selected_pad();
            if selected_pad != current_pad_index {
                current_pad_index = selected_pad;
                let mode = crate::mode::KeyboardMode::from_layer(selected_pad);
                current_mode = mode;
                rmk::set_default_layer(selected_pad);
                // 广播模式变更
                mode_tx.send(mode);
                defmt::info!("Pad switched to {} (layer {})", mode.name(), selected_pad);
            }

            // 实时亮度预览：读取 ValWin 滑块实时值，限速 100ms
            let brightness = wououi.get_live_brightness();
            if brightness != current_brightness {
                if now.duration_since(last_contrast_write) >= CONTRAST_MIN_INTERVAL {
                    current_brightness = brightness;
                    last_contrast_write = now;
                    const MIN_CONTRAST: u16 = 5;
                    let contrast = (MIN_CONTRAST + brightness as u16 * (255 - MIN_CONTRAST) / 100) as u8;
                    if let Err(_) = display.set_contrast(contrast).await {
                        defmt::error!("Failed to set contrast");
                    }
                    defmt::info!("Brightness: {}% (contrast={})", brightness, contrast);
                }
            }

            // 检测 BLE 开关变化
            let ble_enabled = wououi.get_ble_enabled();
            if ble_enabled != current_ble_enabled {
                current_ble_enabled = ble_enabled;
                defmt::info!("BLE enabled: {}", ble_enabled);
                // TODO: RMK 未暴露 BLE 启停的公共 API，待后续支持
            }

            // 检测 User 切换（BLE 多设备）
            let selected_user = wououi.get_selected_user();
            if selected_user != current_user {
                current_user = selected_user;
                rmk::switch_ble_profile(selected_user);
                defmt::info!("User switched to User {} (profile {})", selected_user, selected_user);
            }

            // 检测数据通道配置变化，通知主机
            {
                let dc_enabled = wououi.is_data_channel_enabled(current_pad_index);
                let functions = if dc_enabled {
                    wououi.get_enabled_functions(current_pad_index)
                } else {
                    0
                };
                let new_config = k9_datachannel_proto::PadConfig {
                    active_pad: current_pad_index,
                    enabled_functions: functions,
                };
                static mut PREV_DC_CONFIG: k9_datachannel_proto::PadConfig =
                    k9_datachannel_proto::PadConfig {
                        active_pad: 0xFF,
                        enabled_functions: 0xFFFF,
                    };
                // SAFETY: 单线程 display task 内部使用
                let prev = unsafe { PREV_DC_CONFIG };
                if new_config != prev {
                    unsafe { PREV_DC_CONFIG = new_config };
                    crate::data_channel::DATA_CHANNEL_CONFIG.sender().send(new_config);
                    defmt::info!(
                        "DC config: pad={} functions=0x{:04x}",
                        new_config.active_pad,
                        new_config.enabled_functions
                    );
                }
            }

            // 更新空闲计时器
            menu_idle_ticks += 1;
            if menu_idle_ticks > MENU_TIMEOUT_TICKS {
                menu_active = false;
                wououi.exit_menu();
                defmt::info!("WouoUI: Menu timeout, returning to home");
            }
        } else {
            // 非阻塞消费显示数据命令
            while let Ok(cmd) = DISPLAY_DATA.try_receive() {
                dc_cache.apply(&cmd);
            }

            // 检查当前 Pad 是否启用了数据通道
            let dc_enabled = wououi.is_data_channel_enabled(current_pad_index);
            let active_slots = dc_cache.active_count();

            if dc_enabled && active_slots > 0 {
                // 模式 2：数据通道布局（浮动头部 + 内容区）
                // Slot 轮播：多个 slot 时每 4 秒切换
                if active_slots > 1
                    && now.duration_since(dc_rotation_timer) >= DC_ROTATION_INTERVAL
                {
                    dc_rotation_timer = now;
                    // 找下一个有数据的 slot
                    for _ in 0..8 {
                        dc_current_slot = (dc_current_slot + 1) % 8;
                        if dc_cache.slots[dc_current_slot as usize].is_some() {
                            break;
                        }
                    }
                }

                // 确保当前 slot 有数据（可能被 clear 了）
                if dc_cache.slots[dc_current_slot as usize].is_none() {
                    // 找第一个有数据的 slot
                    for i in 0..8u8 {
                        if dc_cache.slots[i as usize].is_some() {
                            dc_current_slot = i;
                            break;
                        }
                    }
                }

                let slot_data = dc_cache.slots[dc_current_slot as usize].as_ref();
                draw_data_channel_ui(
                    &mut display,
                    current_mode.name(),
                    battery_status.percentage,
                    ble_connected,
                    slot_data,
                );
            } else {
                // 模式 1：居中显示（无数据通道功能启用）
                draw_keyboard_ui(
                    &mut display,
                    current_mode.name(),
                    battery_status.percentage,
                    ble_connected,
                );
            }
        }

        // 刷新到屏幕
        if let Err(_) = display.flush().await {
            defmt::error!("Display flush failed");
        }

        // 广播菜单状态（仅在 active 变化时发送，避免每帧都广播）
        {
            let new_active = menu_active;
            static mut PREV_ACTIVE: bool = false;
            // SAFETY: 单线程 display task 内部使用，无竞争
            let prev = unsafe { PREV_ACTIVE };
            if new_active != prev {
                unsafe { PREV_ACTIVE = new_active };
                // 同步 RMK 菜单模式标志，控制按键/编码器拦截
                crate::menu::set_rmk_menu_mode(new_active);
                let state = MenuState {
                    active: new_active,
                    current_page: if new_active { PageId::MainMenu } else { PageId::Home },
                    selected_index: 0,
                    scroll_offset: 0,
                    target_scroll_offset: 0,
                };
                menu_state_tx.send(state);
            }
        }

        // 动态帧率：菜单模式 120 FPS，首页 1 FPS
        let frame_delay = if menu_active {
            Duration::from_millis(8) // 120 FPS
        } else {
            Duration::from_millis(1000) // 1 FPS
        };

        Timer::after(frame_delay).await;
    }
    }; // end display_future

    // 并行运行显示和菜单控制器
    rmk::embassy_futures::join::join(display_future, menu_ctrl_future).await;
}
