// INPUT:  rmk(KeyEvent, KeyboardEventPos, RotaryEncoder), menu::state, embassy_time
// OUTPUT: menu_controller_task() async task
// POS:    监听 SW1/编码器/确认键 → 转换为 MenuInput 发送到 channel
// menu/controller.rs - 菜单控制器
//
// 订阅 KeyEvent；自行用 embassy_time::Timer 在 firmware 这边做 hold/tap 判定：
// - SW1 短按（<HOLD_THRESHOLD_MS 释放）→ 手动 send_keycode(Kc1) tap 给主机；菜单模式下改发 Back
// - SW1 长按（≥HOLD_THRESHOLD_MS）→ 进入菜单
//
// 历史：上游 RMK 0.10 catchup rebase 把 held_buffer 里的 hold-tap 状态机砍掉了
// （`DeferredEventState::HoldActivated` 不再发射），所以我们改成在 firmware 这边
// 自己跑 timer，不再依赖 RMK 的 deferred_state。

use embassy_time::{Duration, Instant, Timer};
use rmk::controller::KeyEvent;
use rmk::embassy_futures::select::{select, Either};
use rmk::event::{EventSubscriber, KeyPos, KeyboardEventPos, RotaryEncoderPos};
use rmk::input_device::rotary_encoder::Direction;
use rmk::types::keycode::{HidKeyCode, KeyCode};

use super::state::{MenuInput, MENU_INPUT, MENU_STATE};

/// SW1 位置: ROW0/COL3
const SW1_ROW: u8 = 0;
const SW1_COL: u8 = 3;

/// W4B152110 (确认键) 位置: ROW0/COL2
const SELECT_ROW: u8 = 0;
const SELECT_COL: u8 = 2;

/// 编码器 ID
const ENCODER_ID: u8 = 0;

/// 长按阈值（ms）：超过则触发 EnterMenu
const HOLD_THRESHOLD_MS: u64 = 500;

/// 短按时给主机发的按键（与 keyboard.toml ROW0/COL3 = "Kc1" 保持一致）
const SW1_TAP_KEYCODE: KeyCode = KeyCode::Hid(HidKeyCode::Kc1);

/// 菜单控制器
///
/// SW1 用 RMK 的 deferred_key：RMK 不会代发它的 keymap action，我们这里全权处理。
/// - 按下 → 起 timer，跑 select(下个 event, timer)
/// - timer 跑赢 → 长按，发 EnterMenu（仅非菜单模式）
/// - 下个 event 跑赢且是 release → 在 HOLD_THRESHOLD_MS 内释放 = 短按：
///   菜单模式发 Back；非菜单模式手动 send_keycode tap
pub struct MenuController {
    /// 菜单是否激活（缓存）
    menu_active: bool,
    /// 当前 SW1 按压是否已触发 hold（用于忽略 hold 后的 release）
    sw1_hold_activated: bool,
    /// SW1 按下时刻；None 表示未按下（也用作 "需要 race timer" 的信号）
    sw1_pressed_at: Option<Instant>,
}

impl MenuController {
    pub fn new() -> Self {
        // 初始化菜单拦截配置
        super::state::init_menu_intercept();

        Self {
            menu_active: false,
            sw1_hold_activated: false,
            sw1_pressed_at: None,
        }
    }

    /// 主循环：订阅 KeyEvent + 在 SW1 按下时 race hold timer
    pub async fn run(&mut self) -> ! {
        let mut subscriber = rmk::controller::key_event_subscriber()
            .expect("key_event_subscriber: out of slots");

        loop {
            match self.sw1_pressed_at {
                Some(pressed_at) => {
                    // SW1 按着 → race 下个 event 和 hold timer
                    let deadline = pressed_at + Duration::from_millis(HOLD_THRESHOLD_MS);
                    match select(subscriber.next_event(), Timer::at(deadline)).await {
                        Either::First(event) => self.on_key_event(event).await,
                        Either::Second(_) => self.on_sw1_hold_timeout(),
                    }
                }
                None => {
                    // SW1 没按 → 普通阻塞等下个 event
                    let event = subscriber.next_event().await;
                    self.on_key_event(event).await;
                }
            }
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
                self.handle_matrix_key(row, col, keyboard_event.pressed).await;
            }
            KeyboardEventPos::RotaryEncoder(RotaryEncoderPos { id, direction }) => {
                self.handle_encoder(id, direction, keyboard_event.pressed).await;
            }
        }
    }

    /// 处理矩阵按键
    async fn handle_matrix_key(&mut self, row: u8, col: u8, pressed: bool) {
        if row == SW1_ROW && col == SW1_COL {
            self.handle_sw1(pressed).await;
            return;
        }

        if row == SELECT_ROW && col == SELECT_COL {
            self.handle_select(pressed).await;
        }
    }

    /// 处理 SW1（deferred key，RMK 不代发 keymap action）
    async fn handle_sw1(&mut self, pressed: bool) {
        if pressed {
            // 按下：起 hold 计时
            self.sw1_pressed_at = Some(Instant::now());
            self.sw1_hold_activated = false;
            return;
        }

        // 释放
        self.sw1_pressed_at = None;
        let was_hold = self.sw1_hold_activated;
        self.sw1_hold_activated = false;

        if was_hold {
            defmt::info!("Menu: SW1 hold release -> ignored");
            return;
        }

        // 短按：菜单模式发 Back，否则手动 tap SW1_TAP_KEYCODE 给主机
        if self.menu_active {
            let _ = MENU_INPUT.try_send(MenuInput::Back);
            defmt::info!("Menu: SW1 short press -> Back");
        } else {
            rmk::controller::send_keycode(SW1_TAP_KEYCODE, true);
            rmk::controller::send_keycode(SW1_TAP_KEYCODE, false);
            defmt::info!("Menu: SW1 short press -> tap");
        }
    }

    /// hold timer 跑赢 release：长按触发，仅非菜单模式发 EnterMenu
    fn on_sw1_hold_timeout(&mut self) {
        self.sw1_hold_activated = true;
        self.sw1_pressed_at = None; // 不再 race timer，等 release

        if !self.menu_active {
            let _ = MENU_INPUT.try_send(MenuInput::EnterMenu);
            defmt::info!("Menu: SW1 hold -> EnterMenu");
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
