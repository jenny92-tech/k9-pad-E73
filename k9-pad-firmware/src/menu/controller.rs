// INPUT:  rmk(KeyEvent, DeferredEventState, RotaryEncoder), menu::state
// OUTPUT: menu_controller_task() async task
// POS:    监听 SW1/编码器/确认键 → 转换为 MenuInput 发送到 channel
// menu/controller.rs - 菜单控制器
//
// 订阅 KeyEvent，响应 RMK hold-tap deferred key 的决策结果：
// - SW1 短按：RMK 自动 tap ESC（菜单模式下被 should_intercept_key 拦截）
// - SW1 长按：收到 HoldActivated → 进入菜单
// - 菜单模式短按释放：收到 Normal release → 发送 MenuInput::Back

use rmk::event::{EventSubscriber, KeyboardEventPos, KeyPos, RotaryEncoderPos};
use rmk::input_device::rotary_encoder::Direction;
use rmk::{DeferredEventState, KeyEvent};

use super::state::{MenuInput, MENU_INPUT, MENU_STATE};

/// SW1 (ESC) 位置: ROW0/COL3
const SW1_ROW: u8 = 0;
const SW1_COL: u8 = 3;

/// W4B152110 (确认键) 位置: ROW0/COL2
const SELECT_ROW: u8 = 0;
const SELECT_COL: u8 = 2;

/// 编码器 ID
const ENCODER_ID: u8 = 0;

/// 菜单控制器
///
/// SW1 (ESC) 使用 RMK 的 hold-tap deferred key：
/// - 短按（<500ms）→ RMK 自动 tap ESC（菜单模式下被 should_intercept_key 拦截）
/// - 长按（≥500ms）→ RMK 发送 HoldActivated，controller 进入菜单
/// - 菜单模式短按 → RMK tap 被拦截，controller 收到 release 事件发送 Back
pub struct MenuController {
    /// 菜单是否激活（缓存）
    menu_active: bool,
    /// 当前 SW1 按压是否已触发 hold（用于忽略 hold 后的 release）
    sw1_hold_activated: bool,
}

impl MenuController {
    pub fn new() -> Self {
        // 初始化菜单拦截配置
        super::state::init_menu_intercept();

        Self {
            menu_active: false,
            sw1_hold_activated: false,
        }
    }

    /// 主循环：订阅 KeyEvent
    pub async fn run(&mut self) -> ! {
        let mut subscriber = rmk::key_event_subscriber();

        loop {
            let event = subscriber.next_event().await;
            self.on_key_event(event).await;
        }
    }

    /// 处理 KeyEvent
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
                self.handle_matrix_key(row, col, keyboard_event.pressed, &event).await;
            }
            KeyboardEventPos::RotaryEncoder(RotaryEncoderPos { id, direction }) => {
                self.handle_encoder(id, direction, keyboard_event.pressed).await;
            }
        }
    }

    /// 处理矩阵按键
    async fn handle_matrix_key(&mut self, row: u8, col: u8, pressed: bool, event: &KeyEvent) {
        // SW1 (ESC) — hold-tap 由 RMK 处理
        if row == SW1_ROW && col == SW1_COL {
            self.handle_sw1(pressed, event).await;
            return;
        }

        // W4B152110 (确认键)
        if row == SELECT_ROW && col == SELECT_COL {
            self.handle_select(pressed).await;
        }
    }

    /// 处理 SW1 (ESC)
    ///
    /// RMK hold-tap deferred key 处理：
    /// - HoldActivated → 长按：进入菜单
    /// - Normal + release + 菜单激活 → 短按被 should_intercept_key 拦截，发送 Back
    /// - Normal + release + 菜单未激活 → RMK 已自动 tap ESC，无需处理
    async fn handle_sw1(&mut self, pressed: bool, event: &KeyEvent) {
        match event.deferred_state {
            DeferredEventState::HoldActivated => {
                // 长按超时：进入菜单
                self.sw1_hold_activated = true;
                if !self.menu_active {
                    let _ = MENU_INPUT.try_send(MenuInput::EnterMenu);
                    defmt::info!("Menu: SW1 hold -> EnterMenu");
                }
            }
            DeferredEventState::Normal => {
                if pressed {
                    // 新的按压：重置 hold 标记
                    self.sw1_hold_activated = false;
                } else if self.sw1_hold_activated {
                    // hold 后的释放：忽略，不发 Back
                    self.sw1_hold_activated = false;
                    defmt::info!("Menu: SW1 hold release -> ignored");
                } else if self.menu_active {
                    // 菜单模式短按释放：tap 已被 should_intercept_key 拦截，发送 Back
                    let _ = MENU_INPUT.try_send(MenuInput::Back);
                    defmt::info!("Menu: SW1 short press -> Back");
                }
                // 非菜单模式短按：RMK 自动处理 tap，无需干预
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
