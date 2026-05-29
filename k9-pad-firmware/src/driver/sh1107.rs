// INPUT:  embedded_hal_async::i2c, embedded_graphics
// OUTPUT: Sh1107 struct (new, init, flush, set_contrast, clear, DrawTarget impl)
// POS:    SH1107 OLED I2C 驱动，横屏 128x64，脏页追踪刷新

use embedded_graphics::{
    pixelcolor::BinaryColor,
    prelude::*,
};
use embedded_hal_async::i2c::I2c;

use super::board;

const SH1107_ADDR: u8 = board::DISPLAY_I2C_ADDR;
const DISPLAY_WIDTH: u32 = board::DISPLAY_WIDTH;
const DISPLAY_HEIGHT: u32 = board::DISPLAY_HEIGHT;

/// SH1107 显示驱动 (横屏 128x64)
pub struct Sh1107<I2C> {
    pub(crate) i2c: I2C,
    pub(crate) buffer: [u8; 1024],      // 当前帧缓冲区
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
            0xDA, 0x12, // SSD1306 残留：SH1107 无 0xDA 命令(被忽略)；0x12 被当作"设置列高地址"(无害，flush 每页都重设列地址)
            0xDB, 0x35, // Set VCOMH deselect level
            0x20, 0x00, // 0x20 = 页寻址模式(Page Mode, SH1107)——flush() 依赖此模式，必须保留；后面的 0x00 是 SSD1306 残留(被当作"列低地址=0"，无害)
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
    pub fn clear_buffer(&mut self) {
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

    /// 探测 I2C 设备是否存在
    pub async fn probe(&mut self) -> Result<(), I2C::Error> {
        self.i2c.write(SH1107_ADDR, &[0x00]).await
    }
}

// 单独的 impl 块，不需要 I2c trait bound
impl<I2C> Sh1107<I2C> {
    /// 从 WouoUI 缓冲区复制到显示缓冲区
    /// WouoUI 使用 SSD1306 格式（行优先），SH1107 需要列优先（旋转 90°）
    pub fn copy_from_wououi(&mut self, wououi_buffer: &[u8], screen_w: usize, screen_h: usize) {
        self.buffer.fill(0);

        for y in 0..screen_h {
            for x in 0..screen_w {
                let wououi_byte_idx = x + (y / 8) * screen_w;
                let wououi_bit = y % 8;

                if wououi_byte_idx < wououi_buffer.len() {
                    let pixel_on = (wououi_buffer[wououi_byte_idx] & (1 << wououi_bit)) != 0;
                    if pixel_on {
                        let col = y;
                        let row = x;
                        let page = row / 8;
                        let bit = row % 8;
                        let idx = page * 64 + col;
                        if idx < self.buffer.len() {
                            self.buffer[idx] |= 1 << bit;
                        }
                    }
                }
            }
        }
    }

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

    /// Fast whole-buffer clear — O(1) instead of 8192 individual set_pixel calls.
    fn clear(&mut self, color: Self::Color) -> Result<(), Self::Error> {
        self.buffer.fill(if color.is_on() { 0xFF } else { 0x00 });
        Ok(())
    }
}

impl<I2C> OriginDimensions for Sh1107<I2C> {
    fn size(&self) -> Size {
        Size::new(DISPLAY_WIDTH, DISPLAY_HEIGHT)
    }
}
