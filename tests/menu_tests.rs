// tests/menu_tests.rs - 菜单系统单元测试
//
// 独立的测试文件，不依赖嵌入式库
// 直接复制测试所需的纯逻辑结构

// ============== 复制自 menu/state.rs 的纯逻辑部分 ==============

/// 菜单输入事件
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MenuInput {
    ScrollUp,
    ScrollDown,
    Select,
    Back,
    EnterMenu,
    ExitMenu,
}

/// 页面 ID
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum PageId {
    #[default]
    Home,
    MainMenu,
    ModeSelect,
    BleSettings,
    About,
}

/// 菜单状态
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MenuState {
    pub active: bool,
    pub current_page: PageId,
    pub selected_index: u8,
    pub scroll_offset: i16,
    pub target_scroll_offset: i16,
}

impl MenuState {
    pub const fn new() -> Self {
        Self {
            active: false,
            current_page: PageId::Home,
            selected_index: 0,
            scroll_offset: 0,
            target_scroll_offset: 0,
        }
    }

    pub fn reset(&mut self) {
        self.active = false;
        self.current_page = PageId::Home;
        self.selected_index = 0;
        self.scroll_offset = 0;
        self.target_scroll_offset = 0;
    }
}

/// 页面导航栈
#[derive(Debug)]
pub struct NavigationStack {
    stack: [PageId; 4],
    depth: u8,
}

impl Default for NavigationStack {
    fn default() -> Self {
        Self::new()
    }
}

impl NavigationStack {
    pub const fn new() -> Self {
        Self {
            stack: [PageId::MainMenu; 4],
            depth: 0,
        }
    }

    pub fn push(&mut self, page: PageId) {
        if (self.depth as usize) < self.stack.len() {
            self.stack[self.depth as usize] = page;
            self.depth += 1;
        }
    }

    pub fn pop(&mut self) -> Option<PageId> {
        if self.depth > 0 {
            self.depth -= 1;
            Some(self.stack[self.depth as usize])
        } else {
            None
        }
    }

    pub fn current(&self) -> PageId {
        if self.depth > 0 {
            self.stack[(self.depth - 1) as usize]
        } else {
            PageId::MainMenu
        }
    }

    pub fn clear(&mut self) {
        self.depth = 0;
    }

    pub fn is_empty(&self) -> bool {
        self.depth == 0
    }

    pub fn depth(&self) -> u8 {
        self.depth
    }
}

pub const MENU_TIMEOUT_TICKS: u16 = 30 * 30;

/// 菜单状态机
#[derive(Debug)]
pub struct MenuStateMachine {
    pub state: MenuState,
    pub nav_stack: NavigationStack,
    pub idle_ticks: u16,
}

impl Default for MenuStateMachine {
    fn default() -> Self {
        Self::new()
    }
}

impl MenuStateMachine {
    pub const fn new() -> Self {
        Self {
            state: MenuState::new(),
            nav_stack: NavigationStack::new(),
            idle_ticks: 0,
        }
    }

    pub fn process(&mut self, input: MenuInput) -> bool {
        self.idle_ticks = 0;

        match input {
            MenuInput::EnterMenu => self.handle_enter_menu(),
            MenuInput::ExitMenu => self.handle_exit_menu(),
            MenuInput::ScrollUp => self.handle_scroll_up(),
            MenuInput::ScrollDown => self.handle_scroll_down(),
            MenuInput::Select => self.handle_select(),
            MenuInput::Back => self.handle_back(),
        }
    }

    fn handle_enter_menu(&mut self) -> bool {
        if !self.state.active {
            self.state.active = true;
            self.state.current_page = PageId::MainMenu;
            self.state.selected_index = 0;
            self.state.scroll_offset = 0;
            self.nav_stack.clear();
            self.nav_stack.push(PageId::MainMenu);
            return true;
        }
        false
    }

    fn handle_exit_menu(&mut self) -> bool {
        if self.state.active {
            self.state.reset();
            self.nav_stack.clear();
            return true;
        }
        false
    }

    fn handle_scroll_up(&mut self) -> bool {
        if self.state.active && self.state.selected_index > 0 {
            self.state.selected_index -= 1;
            return true;
        }
        false
    }

    fn handle_scroll_down(&mut self) -> bool {
        if self.state.active {
            self.state.selected_index += 1;
            return true;
        }
        false
    }

    fn handle_select(&mut self) -> bool {
        if !self.state.active {
            return false;
        }

        match self.state.current_page {
            PageId::MainMenu => {
                let next_page = match self.state.selected_index {
                    0 => Some(PageId::ModeSelect),
                    1 => Some(PageId::BleSettings),
                    2 => Some(PageId::About),
                    _ => None,
                };
                if let Some(page) = next_page {
                    self.nav_stack.push(page);
                    self.state.current_page = page;
                    self.state.selected_index = 0;
                    self.state.scroll_offset = 0;
                    return true;
                }
            }
            PageId::ModeSelect | PageId::BleSettings => {
                return true;
            }
            _ => {}
        }
        false
    }

    fn handle_back(&mut self) -> bool {
        if !self.state.active {
            return false;
        }

        if self.nav_stack.pop().is_some() {
            if self.nav_stack.is_empty() {
                self.state.reset();
            } else {
                self.state.current_page = self.nav_stack.current();
                self.state.selected_index = 0;
                self.state.scroll_offset = 0;
            }
            return true;
        } else {
            self.state.reset();
            return true;
        }
    }

    pub fn tick(&mut self) -> bool {
        if self.state.active {
            self.idle_ticks += 1;
            if self.idle_ticks > MENU_TIMEOUT_TICKS {
                self.state.reset();
                self.nav_stack.clear();
                return true;
            }
        }
        false
    }

    pub fn get_item_count(&self) -> u8 {
        Self::item_count_for_page(self.state.current_page)
    }

    pub fn item_count_for_page(page: PageId) -> u8 {
        match page {
            PageId::Home => 0,
            PageId::MainMenu => 3,
            PageId::ModeSelect => 3,
            PageId::BleSettings => 2,
            PageId::About => 1,
        }
    }

    pub fn clamp_selection(&mut self) {
        let max = self.get_item_count().saturating_sub(1);
        if self.state.selected_index > max {
            self.state.selected_index = max;
        }
    }

    pub fn is_active(&self) -> bool {
        self.state.active
    }

    pub fn current_page(&self) -> PageId {
        self.state.current_page
    }

    pub fn selected_index(&self) -> u8 {
        self.state.selected_index
    }
}

// ============== 复制自 menu/input.rs 的纯逻辑部分 ==============

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PressType {
    Short,
    Long,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct KeyTrackerTick {
    press_tick: Option<u32>,
    long_press_triggered: bool,
}

impl KeyTrackerTick {
    pub const fn new() -> Self {
        Self {
            press_tick: None,
            long_press_triggered: false,
        }
    }

    pub fn on_press(&mut self, current_tick: u32) {
        self.press_tick = Some(current_tick);
        self.long_press_triggered = false;
    }

    pub fn on_release(&mut self, current_tick: u32, short_press_max_ticks: u32) -> Option<PressType> {
        if let Some(press_tick) = self.press_tick.take() {
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

    pub fn is_pressed(&self) -> bool {
        self.press_tick.is_some()
    }

    pub fn is_long_press_triggered(&self) -> bool {
        self.long_press_triggered
    }
}

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

    pub fn update(&mut self, a: bool, b: bool, current_tick: u32, debounce_ticks: u32) -> Option<bool> {
        if current_tick.saturating_sub(self.last_update_tick) < debounce_ticks {
            return None;
        }

        let old_a = self.last_a;

        self.last_a = a;
        self.last_b = b;
        self.last_update_tick = current_tick;

        if old_a != a {
            if a == b {
                return Some(true);
            } else {
                return Some(false);
            }
        }

        None
    }

    pub fn last_a(&self) -> bool {
        self.last_a
    }

    pub fn last_b(&self) -> bool {
        self.last_b
    }
}

// ============== 复制自 menu/animation.rs 的纯逻辑部分 ==============

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EasingType {
    Linear,
    EaseIn,
    EaseOut,
    EaseInOut,
}

#[derive(Clone, Copy, Debug)]
pub struct Animation {
    start: i16,
    target: i16,
    current: i16,
    duration: u8,
    frame: u8,
    easing: EasingType,
    running: bool,
}

impl Default for Animation {
    fn default() -> Self {
        Self::new()
    }
}

impl Animation {
    pub const fn new() -> Self {
        Self {
            start: 0,
            target: 0,
            current: 0,
            duration: 10,
            frame: 0,
            easing: EasingType::EaseOut,
            running: false,
        }
    }

    pub fn start(&mut self, from: i16, to: i16, duration: u8, easing: EasingType) {
        self.start = from;
        self.target = to;
        self.current = from;
        self.duration = duration.max(1);
        self.frame = 0;
        self.easing = easing;
        self.running = true;
    }

    pub fn update(&mut self) -> i16 {
        if !self.running {
            return self.current;
        }

        self.frame += 1;

        if self.frame >= self.duration {
            self.current = self.target;
            self.running = false;
        } else {
            let t = self.frame as f32 / self.duration as f32;
            let eased_t = self.apply_easing(t);
            let delta = self.target - self.start;
            self.current = self.start + (delta as f32 * eased_t) as i16;
        }

        self.current
    }

    fn apply_easing(&self, t: f32) -> f32 {
        match self.easing {
            EasingType::Linear => t,
            EasingType::EaseIn => t * t,
            EasingType::EaseOut => 1.0 - (1.0 - t) * (1.0 - t),
            EasingType::EaseInOut => {
                if t < 0.5 {
                    2.0 * t * t
                } else {
                    let x = -2.0 * t + 2.0;
                    1.0 - (x * x) / 2.0
                }
            }
        }
    }

    pub fn is_running(&self) -> bool {
        self.running
    }

    pub fn value(&self) -> i16 {
        self.current
    }

    pub fn set_immediate(&mut self, value: i16) {
        self.start = value;
        self.target = value;
        self.current = value;
        self.running = false;
    }
}

#[derive(Clone, Copy)]
pub struct ScrollAnimator {
    pub scroll_y: Animation,
    pub indicator_x: Animation,
    pub indicator_width: Animation,
}

impl Default for ScrollAnimator {
    fn default() -> Self {
        Self::new()
    }
}

impl ScrollAnimator {
    pub const fn new() -> Self {
        Self {
            scroll_y: Animation::new(),
            indicator_x: Animation::new(),
            indicator_width: Animation::new(),
        }
    }

    pub fn update(&mut self) {
        self.scroll_y.update();
        self.indicator_x.update();
        self.indicator_width.update();
    }

    pub fn is_animating(&self) -> bool {
        self.scroll_y.is_running()
            || self.indicator_x.is_running()
            || self.indicator_width.is_running()
    }

    pub fn scroll_to(&mut self, target_y: i16, duration: u8) {
        let current = self.scroll_y.value();
        if current != target_y {
            self.scroll_y.start(current, target_y, duration, EasingType::EaseOut);
        }
    }

    pub fn move_indicator(&mut self, target_x: i16, target_width: i16, duration: u8) {
        let current_x = self.indicator_x.value();
        let current_width = self.indicator_width.value();

        if current_x != target_x {
            self.indicator_x.start(current_x, target_x, duration, EasingType::EaseOut);
        }
        if current_width != target_width {
            self.indicator_width.start(current_width, target_width, duration, EasingType::EaseOut);
        }
    }
}

pub fn lerp_i16(a: i16, b: i16, t_256: u8) -> i16 {
    let delta = (b as i32) - (a as i32);
    let result = (a as i32) + (delta * t_256 as i32 / 256);
    result as i16
}

pub fn ease_out_quad_i16(t_256: u8) -> u8 {
    let inv_t = 255u16 - t_256 as u16;
    let sq = inv_t * inv_t / 255;
    (255 - sq) as u8
}

pub fn ease_in_out_quad_i16(t_256: u8) -> u8 {
    if t_256 < 128 {
        let t2 = (t_256 as u16) * 2;
        (t2 * t2 / 512) as u8
    } else {
        let t2 = (t_256 as u16 - 128) * 2;
        let inv_t = 255 - t2;
        (128 + (127 - inv_t * inv_t / 510)) as u8
    }
}

// ============== 复制自 mode.rs 的纯逻辑部分 ==============

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum KeyboardMode {
    #[default]
    Media,
    Excel,
    Claude,
}

impl KeyboardMode {
    pub const ALL: [KeyboardMode; 3] = [
        KeyboardMode::Media,
        KeyboardMode::Excel,
        KeyboardMode::Claude,
    ];

    pub fn name(&self) -> &'static str {
        match self {
            KeyboardMode::Media => "MEDIA",
            KeyboardMode::Excel => "EXCEL",
            KeyboardMode::Claude => "CLAUDE",
        }
    }

    pub fn next(&self) -> KeyboardMode {
        match self {
            KeyboardMode::Media => KeyboardMode::Excel,
            KeyboardMode::Excel => KeyboardMode::Claude,
            KeyboardMode::Claude => KeyboardMode::Media,
        }
    }

    pub fn prev(&self) -> KeyboardMode {
        match self {
            KeyboardMode::Media => KeyboardMode::Claude,
            KeyboardMode::Excel => KeyboardMode::Media,
            KeyboardMode::Claude => KeyboardMode::Excel,
        }
    }
}

pub fn index_to_mode(index: u8) -> KeyboardMode {
    match index {
        0 => KeyboardMode::Media,
        1 => KeyboardMode::Excel,
        2 => KeyboardMode::Claude,
        _ => KeyboardMode::Media,
    }
}

pub fn mode_to_index(mode: KeyboardMode) -> u8 {
    match mode {
        KeyboardMode::Media => 0,
        KeyboardMode::Excel => 1,
        KeyboardMode::Claude => 2,
    }
}

// ============== 测试 ==============

mod state_tests {
    use super::*;

    #[test]
    fn test_menu_state_new() {
        let state = MenuState::new();
        assert!(!state.active);
        assert_eq!(state.current_page, PageId::Home);
        assert_eq!(state.selected_index, 0);
    }

    #[test]
    fn test_menu_state_reset() {
        let mut state = MenuState {
            active: true,
            current_page: PageId::ModeSelect,
            selected_index: 2,
            scroll_offset: 10,
            target_scroll_offset: 20,
        };
        state.reset();
        assert!(!state.active);
        assert_eq!(state.current_page, PageId::Home);
    }

    #[test]
    fn test_nav_stack_push_pop() {
        let mut stack = NavigationStack::new();
        stack.push(PageId::MainMenu);
        stack.push(PageId::ModeSelect);
        assert_eq!(stack.depth(), 2);
        assert_eq!(stack.pop(), Some(PageId::ModeSelect));
        assert_eq!(stack.pop(), Some(PageId::MainMenu));
        assert!(stack.is_empty());
    }

    #[test]
    fn test_nav_stack_max_depth() {
        let mut stack = NavigationStack::new();
        for _ in 0..10 {
            stack.push(PageId::MainMenu);
        }
        assert_eq!(stack.depth(), 4); // Max depth is 4
    }

    #[test]
    fn test_state_machine_enter_exit() {
        let mut machine = MenuStateMachine::new();

        assert!(machine.process(MenuInput::EnterMenu));
        assert!(machine.is_active());
        assert_eq!(machine.current_page(), PageId::MainMenu);

        assert!(machine.process(MenuInput::ExitMenu));
        assert!(!machine.is_active());
    }

    #[test]
    fn test_state_machine_navigation() {
        let mut machine = MenuStateMachine::new();
        machine.process(MenuInput::EnterMenu);

        // Navigate to ModeSelect
        machine.process(MenuInput::Select);
        assert_eq!(machine.current_page(), PageId::ModeSelect);

        // Go back
        machine.process(MenuInput::Back);
        assert_eq!(machine.current_page(), PageId::MainMenu);
    }

    #[test]
    fn test_state_machine_scroll() {
        let mut machine = MenuStateMachine::new();
        machine.process(MenuInput::EnterMenu);

        assert!(machine.process(MenuInput::ScrollDown));
        assert_eq!(machine.selected_index(), 1);

        assert!(machine.process(MenuInput::ScrollUp));
        assert_eq!(machine.selected_index(), 0);

        // Can't scroll up past 0
        assert!(!machine.process(MenuInput::ScrollUp));
    }

    #[test]
    fn test_idle_timeout() {
        let mut machine = MenuStateMachine::new();
        machine.process(MenuInput::EnterMenu);

        for _ in 0..MENU_TIMEOUT_TICKS + 1 {
            machine.tick();
        }

        assert!(!machine.is_active());
    }

    #[test]
    fn test_clamp_selection() {
        let mut machine = MenuStateMachine::new();
        machine.process(MenuInput::EnterMenu);
        machine.state.selected_index = 100;
        machine.clamp_selection();
        assert_eq!(machine.selected_index(), 2); // MainMenu has 3 items (0, 1, 2)
    }
}

mod input_tests {
    use super::*;

    #[test]
    fn test_key_tracker_short_press() {
        let mut tracker = KeyTrackerTick::new();
        tracker.on_press(0);
        let result = tracker.on_release(100, 300);
        assert_eq!(result, Some(PressType::Short));
    }

    #[test]
    fn test_key_tracker_long_press() {
        let mut tracker = KeyTrackerTick::new();
        tracker.on_press(0);
        assert!(tracker.check_long_press(500, 500));
        assert!(tracker.is_long_press_triggered());

        // Release after long press returns None
        let result = tracker.on_release(600, 300);
        assert_eq!(result, None);
    }

    #[test]
    fn test_encoder_clockwise() {
        let mut tracker = EncoderTrackerTick::new();

        // 01 -> 11 (A changes, A == B) = clockwise
        tracker.update(false, true, 10, 5);
        let result = tracker.update(true, true, 20, 5);
        assert_eq!(result, Some(true));
    }

    #[test]
    fn test_encoder_counter_clockwise() {
        let mut tracker = EncoderTrackerTick::new();

        // 00 -> 10 (A changes, A != B) = counter-clockwise
        let result = tracker.update(true, false, 10, 5);
        assert_eq!(result, Some(false));
    }

    #[test]
    fn test_encoder_debounce() {
        let mut tracker = EncoderTrackerTick::new();
        tracker.update(true, false, 10, 5);

        // Too fast update should be ignored
        let result = tracker.update(true, true, 12, 5);
        assert_eq!(result, None);
    }
}

mod animation_tests {
    use super::*;

    #[test]
    fn test_animation_linear() {
        let mut anim = Animation::new();
        anim.start(0, 100, 10, EasingType::Linear);

        for _ in 0..15 {
            anim.update();
        }

        assert_eq!(anim.value(), 100);
        assert!(!anim.is_running());
    }

    #[test]
    fn test_animation_ease_out_faster_start() {
        let mut anim = Animation::new();
        anim.start(0, 100, 10, EasingType::EaseOut);

        for _ in 0..3 {
            anim.update();
        }

        // EaseOut should be ahead of linear at 30%
        assert!(anim.value() > 30);
    }

    #[test]
    fn test_scroll_animator() {
        let mut animator = ScrollAnimator::new();
        animator.scroll_to(100, 5);

        assert!(animator.is_animating());

        for _ in 0..10 {
            animator.update();
        }

        assert_eq!(animator.scroll_y.value(), 100);
        assert!(!animator.is_animating());
    }

    #[test]
    fn test_lerp() {
        assert_eq!(lerp_i16(0, 100, 0), 0);
        assert!(lerp_i16(0, 100, 128) >= 48 && lerp_i16(0, 100, 128) <= 52);
        assert!(lerp_i16(0, 100, 255) >= 99);
    }

    #[test]
    fn test_ease_out_quad() {
        assert_eq!(ease_out_quad_i16(0), 0);
        assert_eq!(ease_out_quad_i16(255), 255);
        assert!(ease_out_quad_i16(128) > 128); // Faster at start
    }
}

mod mode_tests {
    use super::*;

    #[test]
    fn test_keyboard_mode_default() {
        assert_eq!(KeyboardMode::default(), KeyboardMode::Media);
    }

    #[test]
    fn test_keyboard_mode_name() {
        assert_eq!(KeyboardMode::Media.name(), "MEDIA");
        assert_eq!(KeyboardMode::Excel.name(), "EXCEL");
        assert_eq!(KeyboardMode::Claude.name(), "CLAUDE");
    }

    #[test]
    fn test_keyboard_mode_next_cycle() {
        let mut mode = KeyboardMode::Media;
        mode = mode.next();
        assert_eq!(mode, KeyboardMode::Excel);
        mode = mode.next();
        assert_eq!(mode, KeyboardMode::Claude);
        mode = mode.next();
        assert_eq!(mode, KeyboardMode::Media);
    }

    #[test]
    fn test_keyboard_mode_prev_cycle() {
        let mut mode = KeyboardMode::Media;
        mode = mode.prev();
        assert_eq!(mode, KeyboardMode::Claude);
        mode = mode.prev();
        assert_eq!(mode, KeyboardMode::Excel);
        mode = mode.prev();
        assert_eq!(mode, KeyboardMode::Media);
    }

    #[test]
    fn test_mode_index_conversion() {
        for (i, &mode) in KeyboardMode::ALL.iter().enumerate() {
            assert_eq!(index_to_mode(i as u8), mode);
            assert_eq!(mode_to_index(mode), i as u8);
        }
    }

    #[test]
    fn test_mode_index_out_of_range() {
        assert_eq!(index_to_mode(100), KeyboardMode::Media);
    }
}

mod integration_tests {
    use super::*;

    #[test]
    fn test_full_menu_flow() {
        let mut machine = MenuStateMachine::new();

        // 1. Enter menu
        machine.process(MenuInput::EnterMenu);
        assert!(machine.is_active());
        assert_eq!(machine.current_page(), PageId::MainMenu);

        // 2. Navigate to Mode Select
        machine.process(MenuInput::Select);
        assert_eq!(machine.current_page(), PageId::ModeSelect);

        // 3. Scroll to EXCEL
        machine.process(MenuInput::ScrollDown);
        assert_eq!(machine.selected_index(), 1);
        let mode = index_to_mode(machine.selected_index());
        assert_eq!(mode, KeyboardMode::Excel);

        // 4. Go back
        machine.process(MenuInput::Back);
        assert_eq!(machine.current_page(), PageId::MainMenu);

        // 5. Exit
        machine.process(MenuInput::ExitMenu);
        assert!(!machine.is_active());
    }

    #[test]
    fn test_menu_with_input_tracking() {
        let mut key = KeyTrackerTick::new();
        let mut machine = MenuStateMachine::new();

        // Simulate long press to enter menu
        key.on_press(0);
        assert!(key.check_long_press(500, 500));
        machine.process(MenuInput::EnterMenu);
        assert!(machine.is_active());

        // Simulate encoder scroll
        let mut encoder = EncoderTrackerTick::new();
        encoder.update(false, true, 10, 5);
        if let Some(true) = encoder.update(true, true, 20, 5) {
            machine.process(MenuInput::ScrollDown);
        }
        assert_eq!(machine.selected_index(), 1);

        // Simulate short press to exit
        key.on_press(100);
        if let Some(PressType::Short) = key.on_release(150, 300) {
            machine.process(MenuInput::ExitMenu);
        }
        assert!(!machine.is_active());
    }

    #[test]
    fn test_animation_during_scroll() {
        let mut animator = ScrollAnimator::new();
        let mut machine = MenuStateMachine::new();

        machine.process(MenuInput::EnterMenu);

        // Scroll and animate
        machine.process(MenuInput::ScrollDown);
        animator.scroll_to(machine.selected_index() as i16 * 12, 8);

        // Run animation
        let mut frames = 0;
        while animator.is_animating() && frames < 20 {
            animator.update();
            frames += 1;
        }

        assert_eq!(animator.scroll_y.value(), 12);
    }
}
