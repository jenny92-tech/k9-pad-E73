// menu/state.rs - 菜单状态机和全局状态
//
// 核心数据结构：
// - MenuInput: 菜单输入事件
// - MenuState: 菜单当前状态
// - PageId: 页面标识
// - MenuStateMachine: 状态机处理逻辑
//
// 设计原则：
// - 纯逻辑与硬件依赖分离，便于单元测试
// - 全局状态使用条件编译，测试时不依赖 embassy

#[cfg(not(test))]
use embassy_sync::blocking_mutex::raw::ThreadModeRawMutex;
#[cfg(not(test))]
use embassy_sync::channel::Channel;
#[cfg(not(test))]
use embassy_sync::watch::Watch;

#[cfg(not(test))]
use core::option::Option::{self, None, Some};

#[cfg(not(test))]
use core::sync::atomic::Ordering;

/// 菜单输入事件
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MenuInput {
    /// 编码器逆时针（向上滚动）
    ScrollUp,
    /// 编码器顺时针（向下滚动）
    ScrollDown,
    /// W4B152110 确认键
    Select,
    /// TTC 微动返回
    Back,
    /// 长按 SW1 进入菜单
    EnterMenu,
    /// 短按 SW1 退出菜单
    ExitMenu,
}

/// 页面 ID
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum PageId {
    /// 首页（键盘状态显示）
    #[default]
    Home,
    /// 主菜单
    MainMenu,
    /// 模式选择
    ModeSelect,
    /// 蓝牙设置
    BleSettings,
    /// 关于
    About,
}

/// 菜单状态
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MenuState {
    /// 菜单是否激活
    pub active: bool,
    /// 当前页面
    pub current_page: PageId,
    /// 当前选中项索引
    pub selected_index: u8,
    /// 滚动偏移量（用于长列表）
    pub scroll_offset: i16,
    /// 目标滚动偏移（用于动画）
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

    /// 重置菜单状态
    pub fn reset(&mut self) {
        self.active = false;
        self.current_page = PageId::Home;
        self.selected_index = 0;
        self.scroll_offset = 0;
        self.target_scroll_offset = 0;
    }
}

/// 页面导航栈（最大深度 4）
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

    /// 压入页面
    pub fn push(&mut self, page: PageId) {
        if (self.depth as usize) < self.stack.len() {
            self.stack[self.depth as usize] = page;
            self.depth += 1;
        }
    }

    /// 弹出页面（返回上一页）
    pub fn pop(&mut self) -> Option<PageId> {
        if self.depth > 0 {
            self.depth -= 1;
            Some(self.stack[self.depth as usize])
        } else {
            None
        }
    }

    /// 获取当前页面
    pub fn current(&self) -> PageId {
        if self.depth > 0 {
            self.stack[(self.depth - 1) as usize]
        } else {
            PageId::MainMenu
        }
    }

    /// 清空导航栈
    pub fn clear(&mut self) {
        self.depth = 0;
    }

    /// 是否为空
    pub fn is_empty(&self) -> bool {
        self.depth == 0
    }

    /// 获取栈深度
    pub fn depth(&self) -> u8 {
        self.depth
    }
}

/// 菜单超时配置
pub const MENU_TIMEOUT_TICKS: u16 = 30 * 30; // 30秒 @ 30Hz

/// 菜单状态机
#[derive(Debug)]
pub struct MenuStateMachine {
    pub state: MenuState,
    pub nav_stack: NavigationStack,
    /// 无操作计时器（用于自动退出）
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

    /// 处理输入事件
    /// 返回 true 表示状态发生变化，需要刷新显示
    pub fn process(&mut self, input: MenuInput) -> bool {
        // 重置空闲计时
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

            // 通知 RMK 进入菜单模式，拦截按键
            #[cfg(not(test))]
            set_rmk_menu_mode(true);

            return true;
        }
        false
    }

    fn handle_exit_menu(&mut self) -> bool {
        if self.state.active {
            self.state.reset();
            self.nav_stack.clear();

            // 通知 RMK 退出菜单模式，恢复按键
            #[cfg(not(test))]
            set_rmk_menu_mode(false);

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

    /// 处理选择操作
    fn handle_select(&mut self) -> bool {
        if !self.state.active {
            return false;
        }

        match self.state.current_page {
            PageId::MainMenu => {
                // 主菜单项：0=模式选择, 1=蓝牙设置, 2=关于
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
            PageId::ModeSelect => {
                // 模式选择：选中后应用模式并返回
                // 这里返回 true，让外部处理模式切换
                return true;
            }
            PageId::BleSettings => {
                // 蓝牙设置项处理
                return true;
            }
            _ => {}
        }
        false
    }

    /// 处理返回操作
    fn handle_back(&mut self) -> bool {
        if !self.state.active {
            return false;
        }

        if let Some(_prev_page) = self.nav_stack.pop() {
            // 如果栈空了，退出菜单
            if self.nav_stack.is_empty() {
                self.state.reset();
                // 通知 RMK 退出菜单模式
                #[cfg(not(test))]
                set_rmk_menu_mode(false);
            } else {
                self.state.current_page = self.nav_stack.current();
                self.state.selected_index = 0;
                self.state.scroll_offset = 0;
            }
            return true;
        } else {
            // 栈已空，退出菜单
            self.state.reset();
            // 通知 RMK 退出菜单模式
            #[cfg(not(test))]
            set_rmk_menu_mode(false);
            return true;
        }
    }

    /// 更新空闲计时器
    /// 返回 true 表示超时自动退出
    pub fn tick(&mut self) -> bool {
        if self.state.active {
            self.idle_ticks += 1;
            if self.idle_ticks > MENU_TIMEOUT_TICKS {
                self.state.reset();
                self.nav_stack.clear();
                // 超时退出，通知 RMK 恢复按键
                #[cfg(not(test))]
                set_rmk_menu_mode(false);
                return true;
            }
        }
        false
    }

    /// 获取当前页面的项目数上限
    pub fn get_item_count(&self) -> u8 {
        Self::item_count_for_page(self.state.current_page)
    }

    /// 获取指定页面的项目数
    pub fn item_count_for_page(page: PageId) -> u8 {
        match page {
            PageId::Home => 0,
            PageId::MainMenu => 3,   // 模式、蓝牙、关于
            PageId::ModeSelect => 3, // MEDIA, EXCEL, CLAUDE
            PageId::BleSettings => 2, // 断开、清除配对
            PageId::About => 1,      // 仅显示信息
        }
    }

    /// 限制选中索引不超过项目数
    pub fn clamp_selection(&mut self) {
        let max = self.get_item_count().saturating_sub(1);
        if self.state.selected_index > max {
            self.state.selected_index = max;
        }
    }

    /// 检查是否处于菜单激活状态
    pub fn is_active(&self) -> bool {
        self.state.active
    }

    /// 获取当前页面
    pub fn current_page(&self) -> PageId {
        self.state.current_page
    }

    /// 获取当前选中索引
    pub fn selected_index(&self) -> u8 {
        self.state.selected_index
    }
}

// ============== 全局状态（仅在非测试环境） ==============

#[cfg(not(test))]
/// 输入事件通道（容量 4）
pub static MENU_INPUT: Channel<ThreadModeRawMutex, MenuInput, 4> = Channel::new();

#[cfg(not(test))]
/// 菜单状态广播（供显示任务和其他任务订阅）
pub static MENU_STATE: Watch<ThreadModeRawMutex, MenuState, 2> = Watch::new();

// ============== RMK 菜单拦截集成 ==============

/// 初始化菜单拦截配置（启动时调用一次）
///
/// 配置：
/// - SW1 (ESC): 延迟按键，用于长按/短按检测
/// - W4B152110 (确认键): 拦截按键，菜单模式下不发送
/// - 编码器：拦截，菜单模式下不发送
#[cfg(not(test))]
pub fn init_menu_intercept() {
    // SW1 设为延迟按键（长按/短按检测，由控制器决定是否发送）
    rmk::deferred_key_set(0, 0, 3);  // SW1 (ESC) at ROW0/COL3

    // 确认键设为拦截按键（菜单模式下不发送）
    rmk::menu_intercept_set_key(0, 0, 2);  // W4B152110 at ROW0/COL2

    // 启用编码器拦截
    rmk::MENU_INTERCEPT_ENCODER.store(true, Ordering::Relaxed);

    defmt::info!("Menu: SW1 deferred, Select intercepted, Encoder intercepted");
}

/// 设置 RMK 菜单模式标志
#[cfg(not(test))]
#[inline]
pub fn set_rmk_menu_mode(active: bool) {
    rmk::MENU_MODE_ACTIVE.store(active, Ordering::Relaxed);
}

// ============== 单元测试 ==============

#[cfg(test)]
mod tests {
    use super::*;

    // -------- MenuState 测试 --------

    #[test]
    fn test_menu_state_new() {
        let state = MenuState::new();
        assert!(!state.active);
        assert_eq!(state.current_page, PageId::Home);
        assert_eq!(state.selected_index, 0);
        assert_eq!(state.scroll_offset, 0);
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
        assert_eq!(state.selected_index, 0);
        assert_eq!(state.scroll_offset, 0);
    }

    // -------- NavigationStack 测试 --------

    #[test]
    fn test_nav_stack_new() {
        let stack = NavigationStack::new();
        assert!(stack.is_empty());
        assert_eq!(stack.depth(), 0);
    }

    #[test]
    fn test_nav_stack_push_pop() {
        let mut stack = NavigationStack::new();

        stack.push(PageId::MainMenu);
        assert!(!stack.is_empty());
        assert_eq!(stack.depth(), 1);
        assert_eq!(stack.current(), PageId::MainMenu);

        stack.push(PageId::ModeSelect);
        assert_eq!(stack.depth(), 2);
        assert_eq!(stack.current(), PageId::ModeSelect);

        let popped = stack.pop();
        assert_eq!(popped, Some(PageId::ModeSelect));
        assert_eq!(stack.depth(), 1);
        assert_eq!(stack.current(), PageId::MainMenu);

        let popped = stack.pop();
        assert_eq!(popped, Some(PageId::MainMenu));
        assert!(stack.is_empty());
    }

    #[test]
    fn test_nav_stack_pop_empty() {
        let mut stack = NavigationStack::new();
        assert_eq!(stack.pop(), None);
    }

    #[test]
    fn test_nav_stack_max_depth() {
        let mut stack = NavigationStack::new();

        // 压入 4 个页面（最大深度）
        stack.push(PageId::MainMenu);
        stack.push(PageId::ModeSelect);
        stack.push(PageId::BleSettings);
        stack.push(PageId::About);
        assert_eq!(stack.depth(), 4);

        // 再压入应该被忽略
        stack.push(PageId::Home);
        assert_eq!(stack.depth(), 4);
        assert_eq!(stack.current(), PageId::About);
    }

    #[test]
    fn test_nav_stack_clear() {
        let mut stack = NavigationStack::new();
        stack.push(PageId::MainMenu);
        stack.push(PageId::ModeSelect);
        stack.clear();
        assert!(stack.is_empty());
    }

    // -------- MenuStateMachine 测试 --------

    #[test]
    fn test_state_machine_new() {
        let machine = MenuStateMachine::new();
        assert!(!machine.is_active());
        assert_eq!(machine.current_page(), PageId::Home);
        assert_eq!(machine.idle_ticks, 0);
    }

    #[test]
    fn test_enter_menu() {
        let mut machine = MenuStateMachine::new();

        let changed = machine.process(MenuInput::EnterMenu);
        assert!(changed);
        assert!(machine.is_active());
        assert_eq!(machine.current_page(), PageId::MainMenu);
        assert_eq!(machine.selected_index(), 0);
        assert_eq!(machine.nav_stack.depth(), 1);
    }

    #[test]
    fn test_enter_menu_when_already_active() {
        let mut machine = MenuStateMachine::new();
        machine.process(MenuInput::EnterMenu);

        // 已激活时再次进入应该无变化
        let changed = machine.process(MenuInput::EnterMenu);
        assert!(!changed);
    }

    #[test]
    fn test_exit_menu() {
        let mut machine = MenuStateMachine::new();
        machine.process(MenuInput::EnterMenu);

        let changed = machine.process(MenuInput::ExitMenu);
        assert!(changed);
        assert!(!machine.is_active());
        assert_eq!(machine.current_page(), PageId::Home);
    }

    #[test]
    fn test_exit_menu_when_not_active() {
        let mut machine = MenuStateMachine::new();

        let changed = machine.process(MenuInput::ExitMenu);
        assert!(!changed);
    }

    #[test]
    fn test_scroll_down() {
        let mut machine = MenuStateMachine::new();
        machine.process(MenuInput::EnterMenu);

        let changed = machine.process(MenuInput::ScrollDown);
        assert!(changed);
        assert_eq!(machine.selected_index(), 1);

        machine.process(MenuInput::ScrollDown);
        assert_eq!(machine.selected_index(), 2);
    }

    #[test]
    fn test_scroll_up() {
        let mut machine = MenuStateMachine::new();
        machine.process(MenuInput::EnterMenu);
        machine.process(MenuInput::ScrollDown);
        machine.process(MenuInput::ScrollDown);
        assert_eq!(machine.selected_index(), 2);

        let changed = machine.process(MenuInput::ScrollUp);
        assert!(changed);
        assert_eq!(machine.selected_index(), 1);
    }

    #[test]
    fn test_scroll_up_at_top() {
        let mut machine = MenuStateMachine::new();
        machine.process(MenuInput::EnterMenu);
        assert_eq!(machine.selected_index(), 0);

        // 已在顶部，不能再向上
        let changed = machine.process(MenuInput::ScrollUp);
        assert!(!changed);
        assert_eq!(machine.selected_index(), 0);
    }

    #[test]
    fn test_scroll_when_not_active() {
        let mut machine = MenuStateMachine::new();

        let changed = machine.process(MenuInput::ScrollDown);
        assert!(!changed);

        let changed = machine.process(MenuInput::ScrollUp);
        assert!(!changed);
    }

    #[test]
    fn test_select_navigates_to_mode_select() {
        let mut machine = MenuStateMachine::new();
        machine.process(MenuInput::EnterMenu);
        // 选中索引 0 = ModeSelect

        let changed = machine.process(MenuInput::Select);
        assert!(changed);
        assert_eq!(machine.current_page(), PageId::ModeSelect);
        assert_eq!(machine.nav_stack.depth(), 2);
    }

    #[test]
    fn test_select_navigates_to_ble_settings() {
        let mut machine = MenuStateMachine::new();
        machine.process(MenuInput::EnterMenu);
        machine.process(MenuInput::ScrollDown); // 索引 1 = BleSettings

        let changed = machine.process(MenuInput::Select);
        assert!(changed);
        assert_eq!(machine.current_page(), PageId::BleSettings);
    }

    #[test]
    fn test_select_navigates_to_about() {
        let mut machine = MenuStateMachine::new();
        machine.process(MenuInput::EnterMenu);
        machine.process(MenuInput::ScrollDown);
        machine.process(MenuInput::ScrollDown); // 索引 2 = About

        let changed = machine.process(MenuInput::Select);
        assert!(changed);
        assert_eq!(machine.current_page(), PageId::About);
    }

    #[test]
    fn test_back_returns_to_previous_page() {
        let mut machine = MenuStateMachine::new();
        machine.process(MenuInput::EnterMenu);
        machine.process(MenuInput::Select); // 进入 ModeSelect
        assert_eq!(machine.current_page(), PageId::ModeSelect);

        let changed = machine.process(MenuInput::Back);
        assert!(changed);
        assert_eq!(machine.current_page(), PageId::MainMenu);
    }

    #[test]
    fn test_back_from_main_menu_exits() {
        let mut machine = MenuStateMachine::new();
        machine.process(MenuInput::EnterMenu);
        assert_eq!(machine.current_page(), PageId::MainMenu);

        let changed = machine.process(MenuInput::Back);
        assert!(changed);
        assert!(!machine.is_active());
    }

    #[test]
    fn test_clamp_selection() {
        let mut machine = MenuStateMachine::new();
        machine.process(MenuInput::EnterMenu);

        // 主菜单有 3 项（0, 1, 2）
        machine.state.selected_index = 10;
        machine.clamp_selection();
        assert_eq!(machine.selected_index(), 2);
    }

    #[test]
    fn test_idle_timeout() {
        let mut machine = MenuStateMachine::new();
        machine.process(MenuInput::EnterMenu);
        assert!(machine.is_active());

        // 模拟超时
        for _ in 0..MENU_TIMEOUT_TICKS {
            assert!(!machine.tick());
        }

        // 超时后应该退出
        let timed_out = machine.tick();
        assert!(timed_out);
        assert!(!machine.is_active());
    }

    #[test]
    fn test_input_resets_idle_timer() {
        let mut machine = MenuStateMachine::new();
        machine.process(MenuInput::EnterMenu);

        // 累积一些 idle ticks
        for _ in 0..100 {
            machine.tick();
        }
        assert!(machine.idle_ticks > 0);

        // 任何输入应该重置计时器
        machine.process(MenuInput::ScrollDown);
        assert_eq!(machine.idle_ticks, 0);
    }

    #[test]
    fn test_item_count_for_pages() {
        assert_eq!(MenuStateMachine::item_count_for_page(PageId::Home), 0);
        assert_eq!(MenuStateMachine::item_count_for_page(PageId::MainMenu), 3);
        assert_eq!(MenuStateMachine::item_count_for_page(PageId::ModeSelect), 3);
        assert_eq!(MenuStateMachine::item_count_for_page(PageId::BleSettings), 2);
        assert_eq!(MenuStateMachine::item_count_for_page(PageId::About), 1);
    }

    #[test]
    fn test_navigation_flow_complete() {
        let mut machine = MenuStateMachine::new();

        // 1. 进入菜单
        machine.process(MenuInput::EnterMenu);
        assert!(machine.is_active());
        assert_eq!(machine.current_page(), PageId::MainMenu);

        // 2. 选择 "Mode" (索引 0)
        machine.process(MenuInput::Select);
        assert_eq!(machine.current_page(), PageId::ModeSelect);

        // 3. 滚动到 EXCEL (索引 1)
        machine.process(MenuInput::ScrollDown);
        assert_eq!(machine.selected_index(), 1);

        // 4. 返回主菜单
        machine.process(MenuInput::Back);
        assert_eq!(machine.current_page(), PageId::MainMenu);
        assert_eq!(machine.selected_index(), 0); // 返回后索引重置

        // 5. 选择 "About" (索引 2)
        machine.process(MenuInput::ScrollDown);
        machine.process(MenuInput::ScrollDown);
        machine.process(MenuInput::Select);
        assert_eq!(machine.current_page(), PageId::About);

        // 6. 退出菜单
        machine.process(MenuInput::ExitMenu);
        assert!(!machine.is_active());
    }
}
