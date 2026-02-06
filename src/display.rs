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

use crate::menu::{MenuInput, MENU_INPUT, MENU_STATE, MenuState, PageId};
use crate::mode::CURRENT_MODE;
use crate::battery::BATTERY_STATUS;
use crate::wououi::{WouoUI, WououiInput, SCREEN_WIDTH, SCREEN_HEIGHT};

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
    let _ = Rectangle::new(Point::new(x + 9, y + 2), Size::new(1, 2))
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
    draw_ble_icon(display, 103, 2, ble_connected); // 蓝牙图标
    draw_battery_icon(display, 115, 3, battery_percent); // 电池图标

    // 模式样式
    let title_style = MonoTextStyle::new(&FONT_9X15_BOLD, BinaryColor::On);

    // 绘制模式 (大字居中显示)
    let _ = Text::with_alignment(mode, Point::new(64, 40), title_style, Alignment::Center)
        .draw(display);
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

    // 获取状态发送器和接收器
    let menu_state_tx = MENU_STATE.sender();
    let mut mode_rx = CURRENT_MODE.receiver().unwrap();
    let mut battery_rx = BATTERY_STATUS.receiver().unwrap();

    // 初始状态
    let mut current_mode = mode_rx.try_get().unwrap_or_default();
    let mut battery_status = battery_rx.try_get().unwrap_or_default();
    let ble_connected = false; // TODO: 从实际状态获取

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

    // 主循环
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

        // 更新模式（从 Watch）
        if mode_rx.try_changed().is_some() {
            if let Some(new_mode) = mode_rx.try_get() {
                if new_mode != current_mode {
                    current_mode = new_mode;
                }
            }
        }

        // 更新电池状态
        if battery_rx.try_changed().is_some() {
            if let Some(new_battery) = battery_rx.try_get() {
                battery_status = new_battery;
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

            // 更新空闲计时器
            menu_idle_ticks += 1;
            if menu_idle_ticks > MENU_TIMEOUT_TICKS {
                menu_active = false;
                wououi.exit_menu();
                defmt::info!("WouoUI: Menu timeout, returning to home");
            }
        } else {
            // 首页模式：使用 embedded-graphics 渲染
            draw_keyboard_ui(
                &mut display,
                current_mode.name(),
                battery_status.percentage,
                ble_connected,
            );
        }

        // 刷新到屏幕
        if let Err(_) = display.flush().await {
            defmt::error!("Display flush failed");
        }

        // 广播菜单状态
        let current_state = MenuState {
            active: menu_active,
            current_page: if menu_active { PageId::MainMenu } else { PageId::Home },
            selected_index: 0,
            scroll_offset: 0,
            target_scroll_offset: 0,
        };
        menu_state_tx.send(current_state);

        // 动态帧率：菜单模式 120 FPS，首页 1 FPS
        let frame_delay = if menu_active {
            Duration::from_millis(8) // 120 FPS
        } else {
            Duration::from_millis(1000) // 1 FPS
        };

        Timer::after(frame_delay).await;
    }
}
