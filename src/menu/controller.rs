// menu/controller.rs - 菜单控制器
//
// 使用 RMK 的 controller 机制订阅按键事件
// 在菜单模式下将按键转换为菜单输入
//
// 注意：这个控制器只能**监听**事件，不能**拦截**事件
// 所以在菜单模式下，按键仍会发送键码

use embassy_time::Instant;
use rmk::event::{KeyEvent, KeyboardEventPos, KeyPos, RotaryEncoderPos};
use rmk::input_device::rotary_encoder::Direction;
use rmk::macros::controller;

use super::state::{MenuInput, MENU_INPUT, MENU_STATE};

/// SW1 (ESC) 位置: ROW0/COL3
const SW1_ROW: u8 = 0;
const SW1_COL: u8 = 3;

/// W4B152110 (确认键) 位置: ROW0/COL2
const SELECT_ROW: u8 = 0;
const SELECT_COL: u8 = 2;

/// 编码器 ID
const ENCODER_ID: u8 = 0;

/// 长按阈值 (毫秒)
const LONG_PRESS_MS: u64 = 500;

/// 菜单控制器
///
/// 订阅 KeyEvent，在菜单模式下转换为菜单输入
/// 使用 50ms 轮询间隔检测长按
#[controller(subscribe = [KeyEvent], poll_interval = 50)]
pub struct MenuController {
    /// SW1 按下时间
    sw1_press_time: Option<Instant>,
    /// SW1 是否已触发长按
    sw1_long_triggered: bool,
    /// 菜单是否激活（缓存）
    menu_active: bool,
}

impl MenuController {
    pub fn new() -> Self {
        // 初始化菜单拦截配置
        super::state::init_menu_intercept();

        Self {
            sw1_press_time: None,
            sw1_long_triggered: false,
            menu_active: false,
        }
    }

    /// 处理 KeyEvent（由 controller 宏自动调用）
    async fn on_key_event(&mut self, event: KeyEvent) {
        // 更新菜单状态缓存
        let old_active = self.menu_active;
        if let Some(state) = MENU_STATE.try_get() {
            self.menu_active = state.active;
        }
        if old_active != self.menu_active {
            defmt::info!("Menu active changed: {} -> {}", old_active, self.menu_active);
        }

        let keyboard_event = event.keyboard_event;

        match keyboard_event.pos {
            KeyboardEventPos::Key(KeyPos { row, col }) => {
                self.handle_matrix_key(row, col, keyboard_event.pressed).await;
            }
            KeyboardEventPos::RotaryEncoder(RotaryEncoderPos { id, direction }) => {
                self.handle_encoder(id, direction, keyboard_event.pressed).await;
            }
        }
    }

    /// 处理矩阵按键
    async fn handle_matrix_key(&mut self, row: u8, col: u8, pressed: bool) {
        // SW1 (ESC)
        if row == SW1_ROW && col == SW1_COL {
            self.handle_sw1(pressed).await;
            return;
        }

        // W4B152110 (确认键)
        if row == SELECT_ROW && col == SELECT_COL {
            self.handle_select(pressed).await;
        }
    }

    /// 处理 SW1 (ESC)
    ///
    /// SW1 是延迟按键，由控制器决定是否发送 ESC：
    /// - 短按 + 菜单未激活 → 发送 ESC
    /// - 短按 + 菜单激活 → 菜单返回
    /// - 长按 → 进入菜单（在 poll 中处理）
    async fn handle_sw1(&mut self, pressed: bool) {
        if pressed {
            // 按下：记录时间
            self.sw1_press_time = Some(Instant::now());
            self.sw1_long_triggered = false;
        } else {
            // 释放
            if let Some(press_time) = self.sw1_press_time.take() {
                // 如果已触发长按，不处理释放
                if self.sw1_long_triggered {
                    self.sw1_long_triggered = false;
                    return;
                }

                let duration = press_time.elapsed();
                if duration.as_millis() < 300 {
                    // 短按
                    if self.menu_active {
                        // 菜单模式：返回/退出，不发送 ESC
                        let _ = MENU_INPUT.try_send(MenuInput::Back);
                        defmt::info!("Menu: SW1 short press -> Back");
                    } else {
                        // 正常模式：手动发送 ESC（按下+释放）
                        use rmk::types::keycode::{KeyCode, HidKeyCode};
                        rmk::send_keycode(KeyCode::Hid(HidKeyCode::Escape), true);
                        rmk::send_keycode(KeyCode::Hid(HidKeyCode::Escape), false);
                        defmt::info!("Menu: SW1 short press -> Send ESC");
                    }
                }
                // 长按（300-500ms 之间释放）：什么都不做
            }
        }
    }

    /// 轮询方法（由 controller 宏每 50ms 调用）
    /// 检测 SW1 长按
    async fn poll(&mut self) {
        // 更新菜单状态缓存
        if let Some(state) = MENU_STATE.try_get() {
            self.menu_active = state.active;
        }

        // 检查 SW1 长按
        if let Some(press_time) = self.sw1_press_time {
            if !self.sw1_long_triggered {
                let duration = press_time.elapsed();
                if duration.as_millis() >= LONG_PRESS_MS {
                    self.sw1_long_triggered = true;
                    if !self.menu_active {
                        let _ = MENU_INPUT.try_send(MenuInput::EnterMenu);
                        defmt::info!("Menu: SW1 long press -> EnterMenu");
                    }
                }
            }
        }
    }

    /// 处理确认键
    async fn handle_select(&mut self, pressed: bool) {
        if self.menu_active && pressed {
            let _ = MENU_INPUT.try_send(MenuInput::Select);
            defmt::info!("Menu: Select pressed");
        }
    }

    /// 处理编码器
    async fn handle_encoder(&mut self, id: u8, direction: Direction, pressed: bool) {
        if id != ENCODER_ID || !pressed || direction == Direction::None {
            return;
        }

        if self.menu_active {
            let input = match direction {
                Direction::Clockwise => MenuInput::ScrollUp,
                Direction::CounterClockwise => MenuInput::ScrollDown,
                Direction::None => return,
            };
            let _ = MENU_INPUT.try_send(input);
            defmt::info!("Encoder: {:?}", defmt::Debug2Format(&input));
        }
    }
}
