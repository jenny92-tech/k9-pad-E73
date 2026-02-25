// INPUT:  embassy_sync
// OUTPUT: MenuInput, MenuState, PageId, MENU_INPUT channel, MENU_STATE watch
// POS:    菜单通信类型定义 + 全局 channel/watch（controller → display）
// menu/state.rs - 菜单状态和全局通道
//
// 核心数据结构：
// - MenuInput: 菜单输入事件（controller → display）
// - MenuState: 菜单当前状态（display → controller via Watch）
// - PageId: 页面标识
//
// 菜单逻辑由 WouoUI C 库处理，这里只定义通信用的类型和通道

#[cfg(not(test))]
use embassy_sync::blocking_mutex::raw::ThreadModeRawMutex;
#[cfg(not(test))]
use embassy_sync::channel::Channel;
#[cfg(not(test))]
use embassy_sync::watch::Watch;

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
    /// 蓝牙设置
    BleSettings,
    /// 关于
    About,
}

/// 菜单状态（display 任务广播给 controller 等消费者）
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
    // SW1 设为延迟按键（hold-tap 模式：500ms 阈值）
    // - 短按（<500ms）→ RMK 自动发送 ESC tap
    // - 长按（≥500ms）→ RMK 通知 controller HoldActivated
    rmk::deferred_key_set_with_tap(0, 0, 3, 500);  // SW1 (ESC) at ROW0/COL3

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
