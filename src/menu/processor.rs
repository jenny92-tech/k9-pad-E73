// menu/processor.rs - 菜单输入处理器
//
// 监听 RMK 的按键/编码器事件，在菜单模式下拦截并处理
//
// 按键映射（来自 keyboard.toml）：
// - SW1 (ESC): ROW0/COL3 = (row=0, col=3)
// - W4B152110 (确认): ROW0/COL2 = (row=0, col=2)
// - 编码器: id=0

use embassy_sync::blocking_mutex::raw::ThreadModeRawMutex;
use embassy_sync::channel::Channel;
use embassy_time::Instant;

use rmk::channel::KEY_EVENT_CHANNEL;
use rmk::event::{KeyboardEvent, KeyboardEventPos, KeyPos, RotaryEncoderPos};
use rmk::input_device::rotary_encoder::Direction;

use super::state::{MenuInput, MENU_INPUT, MENU_STATE};

/// SW1 (ESC) 位置: ROW0/COL3
const SW1_ROW: u8 = 0;
const SW1_COL: u8 = 3;

/// W4B152110 (确认键) 位置: ROW0/COL2
const SELECT_ROW: u8 = 0;
const SELECT_COL: u8 = 2;

/// 编码器 ID
const ENCODER_ID: u8 = 0;

/// 长按阈值
const LONG_PRESS_MS: u64 = 500;

/// 需要转发给 RMK 的事件通道
/// 在正常模式下，我们把事件放回这里让 RMK 处理
pub static FORWARDED_KEY_EVENTS: Channel<ThreadModeRawMutex, KeyboardEvent, 8> = Channel::new();

/// 菜单输入处理器状态
pub struct MenuInputProcessor {
    /// SW1 按下时间
    sw1_press_time: Option<Instant>,
    /// SW1 是否已触发长按
    sw1_long_triggered: bool,
    /// 菜单是否激活（缓存）
    menu_active: bool,
}

impl MenuInputProcessor {
    pub const fn new() -> Self {
        Self {
            sw1_press_time: None,
            sw1_long_triggered: false,
            menu_active: false,
        }
    }

    /// 处理按键事件
    /// 返回 true 表示事件已被菜单消费，不应转发给 RMK
    pub fn process_key_event(&mut self, event: KeyboardEvent) -> bool {
        // 更新菜单状态缓存
        if let Some(state) = MENU_STATE.try_get() {
            self.menu_active = state.active;
        }

        match event.pos {
            KeyboardEventPos::Key(KeyPos { row, col }) => {
                self.process_matrix_key(row, col, event.pressed)
            }
            KeyboardEventPos::RotaryEncoder(RotaryEncoderPos { id, direction }) => {
                self.process_encoder(id, direction, event.pressed)
            }
        }
    }

    /// 处理矩阵按键
    fn process_matrix_key(&mut self, row: u8, col: u8, pressed: bool) -> bool {
        // SW1 (ESC) - 长按进入菜单，短按返回/退出
        if row == SW1_ROW && col == SW1_COL {
            return self.handle_sw1(pressed);
        }

        // W4B152110 (确认键) - 仅在菜单模式下拦截
        if row == SELECT_ROW && col == SELECT_COL {
            return self.handle_select(pressed);
        }

        // 其他按键：菜单模式下不拦截（保持正常功能）
        false
    }

    /// 处理 SW1 (ESC)
    fn handle_sw1(&mut self, pressed: bool) -> bool {
        if pressed {
            // 按下：记录时间
            self.sw1_press_time = Some(Instant::now());
            self.sw1_long_triggered = false;
            // 不拦截按下事件，让 RMK 也知道
            false
        } else {
            // 释放
            if let Some(press_time) = self.sw1_press_time.take() {
                // 如果已触发长按，不处理释放
                if self.sw1_long_triggered {
                    self.sw1_long_triggered = false;
                    return true; // 拦截释放事件
                }

                let duration = press_time.elapsed();
                if duration.as_millis() < 300 {
                    // 短按
                    if self.menu_active {
                        // 菜单模式：返回/退出
                        let _ = MENU_INPUT.try_send(MenuInput::Back);
                        return true; // 拦截
                    }
                    // 正常模式：不拦截，让 RMK 发送 ESC
                }
            }
            false
        }
    }

    /// 处理确认键 (W4B152110)
    fn handle_select(&mut self, pressed: bool) -> bool {
        if self.menu_active && pressed {
            // 菜单模式下按下确认键
            let _ = MENU_INPUT.try_send(MenuInput::Select);
            true // 拦截
        } else if self.menu_active {
            // 菜单模式下释放
            true // 也拦截释放
        } else {
            // 正常模式：不拦截
            false
        }
    }

    /// 处理编码器
    fn process_encoder(&mut self, id: u8, direction: Direction, pressed: bool) -> bool {
        if id != ENCODER_ID {
            return false;
        }

        // 编码器事件：pressed=true 表示开始旋转，pressed=false 表示释放
        if !pressed {
            return self.menu_active; // 释放事件：菜单模式下拦截
        }

        if self.menu_active {
            // 菜单模式：发送滚动事件
            let input = match direction {
                Direction::Clockwise => MenuInput::ScrollDown,
                Direction::CounterClockwise => MenuInput::ScrollUp,
                Direction::None => return true,
            };
            let _ = MENU_INPUT.try_send(input);
            true // 拦截
        } else {
            // 正常模式：不拦截
            false
        }
    }

    /// 检查 SW1 长按（在主循环中定期调用）
    pub fn check_sw1_long_press(&mut self) -> bool {
        if let Some(press_time) = self.sw1_press_time {
            if !self.sw1_long_triggered {
                let duration = press_time.elapsed();
                if duration.as_millis() >= LONG_PRESS_MS {
                    self.sw1_long_triggered = true;
                    if !self.menu_active {
                        // 进入菜单
                        let _ = MENU_INPUT.try_send(MenuInput::EnterMenu);
                        return true;
                    }
                }
            }
        }
        false
    }
}

/// 菜单输入处理任务
///
/// 从 KEY_EVENT_CHANNEL 接收事件，处理菜单输入
/// 非菜单相关的事件转发到 FORWARDED_KEY_EVENTS
#[embassy_executor::task]
pub async fn menu_input_task() {
    let mut processor = MenuInputProcessor::new();

    defmt::info!("Menu input processor started");

    loop {
        // 接收按键事件
        let event = KEY_EVENT_CHANNEL.receive().await;

        // 定期检查长按
        processor.check_sw1_long_press();

        // 处理事件
        let consumed = processor.process_key_event(event);

        if !consumed {
            // 事件未被菜单消费，转发给下游处理
            // 注意：这里需要与 RMK 的键盘处理集成
            // 目前先记录日志
            defmt::trace!("Forwarding key event: {:?}", defmt::Debug2Format(&event));
        }
    }
}
