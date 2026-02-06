// menu/input.rs - 输入处理
//
// 负责：
// - SW1 长按检测（进入菜单）
// - SW1 短按检测（退出菜单）
// - 编码器旋转检测
// - 按键拦截（菜单模式下不发送键码）
//
// 设计原则：
// - 核心逻辑与时间依赖分离，便于单元测试
// - 使用 tick 计数代替绝对时间，更易测试

#[cfg(not(test))]
use embassy_time::Instant;

#[cfg(not(test))]
use super::state::MENU_INPUT;

use super::state::MenuInput;

/// 长按检测阈值（毫秒）
pub const LONG_PRESS_THRESHOLD_MS: u64 = 500;

/// 短按最大时长（毫秒）
pub const SHORT_PRESS_MAX_MS: u64 = 300;

/// 编码器去抖动时间（毫秒）
pub const ENCODER_DEBOUNCE_MS: u64 = 5;

/// 按键类型
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PressType {
    Short,
    Long,
}

/// 按键状态追踪（基于 tick 计数，可测试版本）
#[derive(Clone, Copy, Debug, Default)]
pub struct KeyTrackerTick {
    /// 按下时的 tick 计数（None 表示未按下）
    press_tick: Option<u32>,
    /// 是否已触发长按
    long_press_triggered: bool,
}

impl KeyTrackerTick {
    pub const fn new() -> Self {
        Self {
            press_tick: None,
            long_press_triggered: false,
        }
    }

    /// 按键按下，记录当前 tick
    pub fn on_press(&mut self, current_tick: u32) {
        self.press_tick = Some(current_tick);
        self.long_press_triggered = false;
    }

    /// 按键释放，返回按键类型
    /// `current_tick`: 当前 tick 计数
    /// `short_press_max_ticks`: 短按最大 tick 数
    pub fn on_release(&mut self, current_tick: u32, short_press_max_ticks: u32) -> Option<PressType> {
        if let Some(press_tick) = self.press_tick.take() {
            // 如果已触发长按，不再处理释放事件
            if self.long_press_triggered {
                self.long_press_triggered = false;
                return None;
            }

            let duration = current_tick.saturating_sub(press_tick);
            if duration < short_press_max_ticks {
                return Some(PressType::Short);
            }
        }
        None
    }

    /// 检查是否应触发长按
    /// `current_tick`: 当前 tick 计数
    /// `long_press_threshold_ticks`: 长按阈值 tick 数
    pub fn check_long_press(&mut self, current_tick: u32, long_press_threshold_ticks: u32) -> bool {
        if let Some(press_tick) = self.press_tick {
            if !self.long_press_triggered {
                let duration = current_tick.saturating_sub(press_tick);
                if duration >= long_press_threshold_ticks {
                    self.long_press_triggered = true;
                    return true;
                }
            }
        }
        false
    }

    /// 是否正在按下
    pub fn is_pressed(&self) -> bool {
        self.press_tick.is_some()
    }

    /// 是否已触发长按
    pub fn is_long_press_triggered(&self) -> bool {
        self.long_press_triggered
    }
}

/// 编码器状态追踪（基于 tick 计数，可测试版本）
#[derive(Clone, Copy, Debug, Default)]
pub struct EncoderTrackerTick {
    last_a: bool,
    last_b: bool,
    last_update_tick: u32,
}

impl EncoderTrackerTick {
    pub const fn new() -> Self {
        Self {
            last_a: false,
            last_b: false,
            last_update_tick: 0,
        }
    }

    /// 更新编码器状态，返回旋转方向
    /// 返回值：Some(true) = 顺时针, Some(false) = 逆时针, None = 无变化
    pub fn update(&mut self, a: bool, b: bool, current_tick: u32, debounce_ticks: u32) -> Option<bool> {
        // 去抖动
        if current_tick.saturating_sub(self.last_update_tick) < debounce_ticks {
            return None;
        }

        let old_a = self.last_a;

        self.last_a = a;
        self.last_b = b;
        self.last_update_tick = current_tick;

        // 使用格雷码检测旋转方向
        // 顺时针: 00 -> 01 -> 11 -> 10 -> 00
        // 逆时针: 00 -> 10 -> 11 -> 01 -> 00
        if old_a != a {
            // A 相变化
            if a == b {
                // 顺时针
                return Some(true);
            } else {
                // 逆时针
                return Some(false);
            }
        }

        None
    }

    /// 获取最后的 A 相状态
    pub fn last_a(&self) -> bool {
        self.last_a
    }

    /// 获取最后的 B 相状态
    pub fn last_b(&self) -> bool {
        self.last_b
    }
}

// ============== 硬件依赖版本（仅非测试环境） ==============

#[cfg(not(test))]
/// 按键状态追踪（使用 embassy_time::Instant）
#[derive(Clone, Copy, Default)]
pub struct KeyTracker {
    /// 按下时间戳
    press_time: Option<Instant>,
    /// 是否已触发长按
    long_press_triggered: bool,
}

#[cfg(not(test))]
impl KeyTracker {
    pub const fn new() -> Self {
        Self {
            press_time: None,
            long_press_triggered: false,
        }
    }

    /// 按键按下
    pub fn on_press(&mut self) {
        self.press_time = Some(Instant::now());
        self.long_press_triggered = false;
    }

    /// 按键释放，返回按键类型
    pub fn on_release(&mut self) -> Option<PressType> {
        if let Some(press_time) = self.press_time.take() {
            let duration = press_time.elapsed();

            // 如果已触发长按，不再处理释放事件
            if self.long_press_triggered {
                self.long_press_triggered = false;
                return None;
            }

            if duration.as_millis() < SHORT_PRESS_MAX_MS {
                return Some(PressType::Short);
            }
        }
        None
    }

    /// 检查是否应触发长按（在按住期间调用）
    pub fn check_long_press(&mut self) -> bool {
        if let Some(press_time) = self.press_time {
            if !self.long_press_triggered {
                let duration = press_time.elapsed();
                if duration.as_millis() >= LONG_PRESS_THRESHOLD_MS {
                    self.long_press_triggered = true;
                    return true;
                }
            }
        }
        false
    }

    /// 是否正在按下
    pub fn is_pressed(&self) -> bool {
        self.press_time.is_some()
    }
}

#[cfg(not(test))]
/// 编码器状态追踪
pub struct EncoderTracker {
    last_a: bool,
    last_b: bool,
    last_update: Instant,
}

#[cfg(not(test))]
impl EncoderTracker {
    pub const fn new() -> Self {
        Self {
            last_a: false,
            last_b: false,
            last_update: Instant::MIN,
        }
    }

    /// 更新编码器状态，返回旋转方向
    /// 返回值：Some(true) = 顺时针, Some(false) = 逆时针, None = 无变化
    pub fn update(&mut self, a: bool, b: bool) -> Option<bool> {
        let now = Instant::now();

        // 去抖动
        if now.duration_since(self.last_update).as_millis() < ENCODER_DEBOUNCE_MS {
            return None;
        }

        let old_a = self.last_a;

        self.last_a = a;
        self.last_b = b;
        self.last_update = now;

        // 使用格雷码检测旋转方向
        if old_a != a {
            if a == b {
                return Some(true); // 顺时针
            } else {
                return Some(false); // 逆时针
            }
        }

        None
    }
}

#[cfg(not(test))]
/// 输入处理器
/// 管理菜单相关输入的状态
///
/// 只占用 3 个输入：
/// - SW1 (ESC): 长按进入菜单，短按返回/退出
/// - 编码器: 滚动菜单
/// - W4B152110: 确认选择
pub struct InputHandler {
    /// SW1 (ESC) 按键追踪
    pub sw1_tracker: KeyTracker,
    /// 确认键 (W4B152110) 追踪
    pub select_tracker: KeyTracker,
    /// 编码器追踪
    pub encoder_tracker: EncoderTracker,
    /// 菜单是否激活
    pub menu_active: bool,
}

#[cfg(not(test))]
impl InputHandler {
    pub const fn new() -> Self {
        Self {
            sw1_tracker: KeyTracker::new(),
            select_tracker: KeyTracker::new(),
            encoder_tracker: EncoderTracker::new(),
            menu_active: false,
        }
    }

    /// 处理 SW1 按下事件
    pub async fn on_sw1_press(&mut self) {
        self.sw1_tracker.on_press();
    }

    /// 处理 SW1 释放事件
    /// 菜单模式下短按：发送 Back（状态机会判断是返回上级还是退出）
    pub async fn on_sw1_release(&mut self) {
        if let Some(press_type) = self.sw1_tracker.on_release() {
            match press_type {
                PressType::Short => {
                    if self.menu_active {
                        // 短按发送 Back，状态机根据当前页面决定是返回还是退出
                        let _ = MENU_INPUT.try_send(MenuInput::Back);
                    }
                    // 正常模式下不处理，由 RMK 发送 ESC 键码
                }
                PressType::Long => {
                    // 长按已在 check 中处理
                }
            }
        }
    }

    /// 定期检查 SW1 长按（在主循环中调用）
    pub async fn check_sw1_long_press(&mut self) {
        if self.sw1_tracker.check_long_press() {
            // 长按：进入菜单
            if !self.menu_active {
                let _ = MENU_INPUT.try_send(MenuInput::EnterMenu);
            }
        }
    }

    /// 处理编码器变化
    pub async fn on_encoder_change(&mut self, a: bool, b: bool) {
        if let Some(clockwise) = self.encoder_tracker.update(a, b) {
            if self.menu_active {
                let input = if clockwise {
                    MenuInput::ScrollDown
                } else {
                    MenuInput::ScrollUp
                };
                let _ = MENU_INPUT.try_send(input);
            }
        }
    }

    /// 处理确认键按下
    pub async fn on_select_press(&mut self) {
        if self.menu_active {
            let _ = MENU_INPUT.try_send(MenuInput::Select);
        }
    }

    /// 更新菜单激活状态
    pub fn update_menu_state(&mut self, active: bool) {
        self.menu_active = active;
    }

    /// 检查是否应该拦截按键（菜单模式下不发送键码）
    pub fn should_intercept_key(&self) -> bool {
        self.menu_active
    }
}

#[cfg(not(test))]
/// 发送菜单输入事件的辅助函数
pub async fn send_menu_input(input: MenuInput) {
    MENU_INPUT.send(input).await;
}

#[cfg(not(test))]
/// 尝试发送菜单输入事件（非阻塞）
pub fn try_send_menu_input(input: MenuInput) -> bool {
    MENU_INPUT.try_send(input).is_ok()
}

// ============== 单元测试 ==============

#[cfg(test)]
mod tests {
    use super::*;

    // -------- KeyTrackerTick 测试 --------

    #[test]
    fn test_key_tracker_new() {
        let tracker = KeyTrackerTick::new();
        assert!(!tracker.is_pressed());
        assert!(!tracker.is_long_press_triggered());
    }

    #[test]
    fn test_key_tracker_press() {
        let mut tracker = KeyTrackerTick::new();
        tracker.on_press(100);
        assert!(tracker.is_pressed());
    }

    #[test]
    fn test_key_tracker_short_press() {
        let mut tracker = KeyTrackerTick::new();

        // 按下
        tracker.on_press(100);

        // 短时间后释放（50 ticks < 300 ticks 阈值）
        let result = tracker.on_release(150, 300);
        assert_eq!(result, Some(PressType::Short));
        assert!(!tracker.is_pressed());
    }

    #[test]
    fn test_key_tracker_not_short_press_when_too_long() {
        let mut tracker = KeyTrackerTick::new();

        // 按下
        tracker.on_press(100);

        // 长时间后释放（400 ticks >= 300 ticks 阈值）
        let result = tracker.on_release(500, 300);
        assert_eq!(result, None); // 不是短按
    }

    #[test]
    fn test_key_tracker_long_press() {
        let mut tracker = KeyTrackerTick::new();

        // 按下
        tracker.on_press(100);

        // 检查长按（200 ticks < 500 ticks 阈值）
        assert!(!tracker.check_long_press(300, 500));
        assert!(!tracker.is_long_press_triggered());

        // 检查长按（500 ticks >= 500 ticks 阈值）
        assert!(tracker.check_long_press(600, 500));
        assert!(tracker.is_long_press_triggered());
    }

    #[test]
    fn test_key_tracker_long_press_only_triggers_once() {
        let mut tracker = KeyTrackerTick::new();

        tracker.on_press(100);

        // 第一次触发长按
        assert!(tracker.check_long_press(600, 500));

        // 第二次不再触发
        assert!(!tracker.check_long_press(700, 500));
    }

    #[test]
    fn test_key_tracker_release_after_long_press_returns_none() {
        let mut tracker = KeyTrackerTick::new();

        tracker.on_press(100);
        tracker.check_long_press(600, 500); // 触发长按

        // 释放后应该返回 None（因为已经触发了长按）
        let result = tracker.on_release(700, 300);
        assert_eq!(result, None);
    }

    #[test]
    fn test_key_tracker_release_without_press() {
        let mut tracker = KeyTrackerTick::new();

        // 没有按下就释放
        let result = tracker.on_release(100, 300);
        assert_eq!(result, None);
    }

    // -------- EncoderTrackerTick 测试 --------

    #[test]
    fn test_encoder_tracker_new() {
        let tracker = EncoderTrackerTick::new();
        assert!(!tracker.last_a());
        assert!(!tracker.last_b());
    }

    #[test]
    fn test_encoder_clockwise() {
        let mut tracker = EncoderTrackerTick::new();

        // 初始状态 00
        // 顺时针: 00 -> 01 -> 11 -> 10 -> 00

        // 00 -> 01 (A: false->false, B: false->true) - A 没变，无输出
        let result = tracker.update(false, true, 10, 5);
        assert_eq!(result, None);

        // 01 -> 11 (A: false->true, B: true) - A 变了，A==B，顺时针
        let result = tracker.update(true, true, 20, 5);
        assert_eq!(result, Some(true)); // 顺时针
    }

    #[test]
    fn test_encoder_counter_clockwise() {
        let mut tracker = EncoderTrackerTick::new();

        // 初始状态 00
        // 逆时针: 00 -> 10 -> 11 -> 01 -> 00

        // 00 -> 10 (A: false->true, B: false) - A 变了，A!=B，逆时针
        let result = tracker.update(true, false, 10, 5);
        assert_eq!(result, Some(false)); // 逆时针
    }

    #[test]
    fn test_encoder_debounce() {
        let mut tracker = EncoderTrackerTick::new();

        // 第一次更新
        tracker.update(true, false, 10, 5);

        // 太快的第二次更新应该被忽略（3 < 5 去抖动阈值）
        let result = tracker.update(true, true, 13, 5);
        assert_eq!(result, None);

        // 足够间隔后的更新应该有效
        let result = tracker.update(true, true, 20, 5);
        // 这里 A 没变（还是 true），所以返回 None
        assert_eq!(result, None);
    }

    #[test]
    fn test_encoder_full_rotation_clockwise() {
        let mut tracker = EncoderTrackerTick::new();
        let mut tick = 0u32;
        let debounce = 5u32;

        // 完整顺时针旋转: 00 -> 01 -> 11 -> 10 -> 00
        let states = [(false, false), (false, true), (true, true), (true, false), (false, false)];
        let mut cw_count = 0;
        let mut ccw_count = 0;

        for (a, b) in states.iter() {
            tick += 10;
            if let Some(clockwise) = tracker.update(*a, *b, tick, debounce) {
                if clockwise {
                    cw_count += 1;
                } else {
                    ccw_count += 1;
                }
            }
        }

        // 顺时针旋转应该产生顺时针事件
        assert!(cw_count > 0);
        assert_eq!(ccw_count, 0);
    }

    #[test]
    fn test_encoder_full_rotation_counter_clockwise() {
        let mut tracker = EncoderTrackerTick::new();
        let mut tick = 0u32;
        let debounce = 5u32;

        // 完整逆时针旋转: 00 -> 10 -> 11 -> 01 -> 00
        let states = [(false, false), (true, false), (true, true), (false, true), (false, false)];
        let mut cw_count = 0;
        let mut ccw_count = 0;

        for (a, b) in states.iter() {
            tick += 10;
            if let Some(clockwise) = tracker.update(*a, *b, tick, debounce) {
                if clockwise {
                    cw_count += 1;
                } else {
                    ccw_count += 1;
                }
            }
        }

        // 逆时针旋转应该产生逆时针事件
        assert!(ccw_count > 0);
        assert_eq!(cw_count, 0);
    }

    // -------- 集成场景测试 --------

    #[test]
    fn test_menu_entry_flow() {
        // 模拟用户长按 SW1 进入菜单的流程
        let mut key_tracker = KeyTrackerTick::new();
        let long_press_ticks = 500; // 500ms @ 1ms/tick

        // 用户按下按键
        key_tracker.on_press(0);

        // 持续按住...每 10ms 检查一次
        for tick in (10..500).step_by(10) {
            assert!(!key_tracker.check_long_press(tick, long_press_ticks));
        }

        // 500ms 后触发长按
        assert!(key_tracker.check_long_press(500, long_press_ticks));

        // 用户释放按键
        let result = key_tracker.on_release(600, 300);
        assert_eq!(result, None); // 已触发长按，释放不产生短按
    }

    #[test]
    fn test_menu_exit_flow() {
        // 模拟用户短按 SW1 退出菜单的流程
        let mut key_tracker = KeyTrackerTick::new();
        let short_press_max_ticks = 300;

        // 用户按下按键
        key_tracker.on_press(0);

        // 快速释放（100ms）
        let result = key_tracker.on_release(100, short_press_max_ticks);
        assert_eq!(result, Some(PressType::Short));
    }

    #[test]
    fn test_encoder_scroll_sequence() {
        // 模拟用户快速旋转编码器滚动菜单
        let mut encoder = EncoderTrackerTick::new();
        let debounce = 5;
        let mut tick = 0u32;
        let mut scroll_count = 0;

        // 模拟连续顺时针旋转 3 次
        for _ in 0..3 {
            // 00 -> 01
            tick += 10;
            encoder.update(false, true, tick, debounce);
            // 01 -> 11 (触发顺时针)
            tick += 10;
            if let Some(true) = encoder.update(true, true, tick, debounce) {
                scroll_count += 1;
            }
            // 11 -> 10
            tick += 10;
            encoder.update(true, false, tick, debounce);
            // 10 -> 00 (触发顺时针)
            tick += 10;
            if let Some(true) = encoder.update(false, false, tick, debounce) {
                scroll_count += 1;
            }
        }

        // 应该检测到多次顺时针旋转
        assert!(scroll_count > 0);
    }
}
