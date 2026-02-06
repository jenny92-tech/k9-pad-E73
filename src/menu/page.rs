// menu/page.rs - 页面定义
//
// 静态页面内容定义，包括菜单项、图标、标题等

use super::state::PageId;
use crate::mode::KeyboardMode;

/// 菜单项定义
#[derive(Clone, Copy)]
pub struct MenuItem {
    /// 显示文本
    pub label: &'static str,
    /// 图标（可选，用于后续扩展）
    pub icon: Option<MenuIcon>,
    /// 目标页面（如果是导航项）
    pub target: Option<PageId>,
    /// 是否可选中
    pub selectable: bool,
}

impl MenuItem {
    pub const fn new(label: &'static str) -> Self {
        Self {
            label,
            icon: None,
            target: None,
            selectable: true,
        }
    }

    pub const fn with_target(label: &'static str, target: PageId) -> Self {
        Self {
            label,
            icon: None,
            target: Some(target),
            selectable: true,
        }
    }

    pub const fn info(label: &'static str) -> Self {
        Self {
            label,
            icon: None,
            target: None,
            selectable: false,
        }
    }
}

/// 菜单图标类型（简单像素图标）
#[derive(Clone, Copy)]
pub enum MenuIcon {
    Mode,
    Bluetooth,
    Info,
    Check,
    Back,
}

/// 页面内容
pub struct PageContent {
    /// 页面标题
    pub title: &'static str,
    /// 菜单项列表
    pub items: &'static [MenuItem],
    /// 是否显示返回选项
    pub show_back: bool,
}

impl PageContent {
    pub const fn new(title: &'static str, items: &'static [MenuItem]) -> Self {
        Self {
            title,
            items,
            show_back: true,
        }
    }

    pub const fn without_back(title: &'static str, items: &'static [MenuItem]) -> Self {
        Self {
            title,
            items,
            show_back: false,
        }
    }
}

// ============== 静态页面定义 ==============

/// 主菜单项
pub static MAIN_MENU_ITEMS: &[MenuItem] = &[
    MenuItem::with_target("Mode", PageId::ModeSelect),
    MenuItem::with_target("Bluetooth", PageId::BleSettings),
    MenuItem::with_target("About", PageId::About),
];

/// 主菜单页面
pub static MAIN_MENU_PAGE: PageContent = PageContent::without_back("Menu", MAIN_MENU_ITEMS);

/// 模式选择项
pub static MODE_SELECT_ITEMS: &[MenuItem] = &[
    MenuItem::new("MEDIA"),
    MenuItem::new("EXCEL"),
    MenuItem::new("CLAUDE"),
];

/// 模式选择页面
pub static MODE_SELECT_PAGE: PageContent = PageContent::new("Mode", MODE_SELECT_ITEMS);

/// 蓝牙设置项
pub static BLE_SETTINGS_ITEMS: &[MenuItem] = &[
    MenuItem::new("Disconnect"),
    MenuItem::new("Clear Pairing"),
];

/// 蓝牙设置页面
pub static BLE_SETTINGS_PAGE: PageContent = PageContent::new("Bluetooth", BLE_SETTINGS_ITEMS);

/// 关于页面项（显示信息）
pub static ABOUT_ITEMS: &[MenuItem] = &[
    MenuItem::info("K9-Pad E73"),
    MenuItem::info("FW: v0.2.0"),
    MenuItem::info("RMK Based"),
];

/// 关于页面
pub static ABOUT_PAGE: PageContent = PageContent::new("About", ABOUT_ITEMS);

/// 根据 PageId 获取页面内容
pub fn get_page_content(page_id: PageId) -> Option<&'static PageContent> {
    match page_id {
        PageId::Home => None,
        PageId::MainMenu => Some(&MAIN_MENU_PAGE),
        PageId::ModeSelect => Some(&MODE_SELECT_PAGE),
        PageId::BleSettings => Some(&BLE_SETTINGS_PAGE),
        PageId::About => Some(&ABOUT_PAGE),
    }
}

/// 将模式选择索引转换为 KeyboardMode
pub fn index_to_mode(index: u8) -> KeyboardMode {
    match index {
        0 => KeyboardMode::Media,
        1 => KeyboardMode::Excel,
        2 => KeyboardMode::Claude,
        _ => KeyboardMode::Media,
    }
}

/// 将 KeyboardMode 转换为选择索引
pub fn mode_to_index(mode: KeyboardMode) -> u8 {
    match mode {
        KeyboardMode::Media => 0,
        KeyboardMode::Excel => 1,
        KeyboardMode::Claude => 2,
    }
}

// ============== 单元测试 ==============

#[cfg(test)]
mod tests {
    use super::*;

    // -------- MenuItem 测试 --------

    #[test]
    fn test_menu_item_new() {
        let item = MenuItem::new("Test");
        assert_eq!(item.label, "Test");
        assert!(item.icon.is_none());
        assert!(item.target.is_none());
        assert!(item.selectable);
    }

    #[test]
    fn test_menu_item_with_target() {
        let item = MenuItem::with_target("Mode", PageId::ModeSelect);
        assert_eq!(item.label, "Mode");
        assert_eq!(item.target, Some(PageId::ModeSelect));
        assert!(item.selectable);
    }

    #[test]
    fn test_menu_item_info() {
        let item = MenuItem::info("Version");
        assert_eq!(item.label, "Version");
        assert!(!item.selectable);
    }

    // -------- PageContent 测试 --------

    #[test]
    fn test_page_content_new() {
        static TEST_ITEMS: &[MenuItem] = &[MenuItem::new("Item1")];
        let page = PageContent::new("Test", TEST_ITEMS);
        assert_eq!(page.title, "Test");
        assert!(page.show_back);
        assert_eq!(page.items.len(), 1);
    }

    #[test]
    fn test_page_content_without_back() {
        static TEST_ITEMS: &[MenuItem] = &[MenuItem::new("Item1")];
        let page = PageContent::without_back("Test", TEST_ITEMS);
        assert!(!page.show_back);
    }

    // -------- 静态页面测试 --------

    #[test]
    fn test_main_menu_page() {
        assert_eq!(MAIN_MENU_PAGE.title, "Menu");
        assert!(!MAIN_MENU_PAGE.show_back);
        assert_eq!(MAIN_MENU_PAGE.items.len(), 3);

        // 验证菜单项
        assert_eq!(MAIN_MENU_ITEMS[0].label, "Mode");
        assert_eq!(MAIN_MENU_ITEMS[0].target, Some(PageId::ModeSelect));
        assert_eq!(MAIN_MENU_ITEMS[1].label, "Bluetooth");
        assert_eq!(MAIN_MENU_ITEMS[2].label, "About");
    }

    #[test]
    fn test_mode_select_page() {
        assert_eq!(MODE_SELECT_PAGE.title, "Mode");
        assert!(MODE_SELECT_PAGE.show_back);
        assert_eq!(MODE_SELECT_PAGE.items.len(), 3);

        // 所有项目应该是可选择的
        for item in MODE_SELECT_ITEMS {
            assert!(item.selectable);
        }
    }

    #[test]
    fn test_ble_settings_page() {
        assert_eq!(BLE_SETTINGS_PAGE.title, "Bluetooth");
        assert_eq!(BLE_SETTINGS_PAGE.items.len(), 2);
    }

    #[test]
    fn test_about_page() {
        assert_eq!(ABOUT_PAGE.title, "About");
        assert_eq!(ABOUT_PAGE.items.len(), 3);

        // 关于页面的项目不应该是可选择的
        for item in ABOUT_ITEMS {
            assert!(!item.selectable);
        }
    }

    // -------- get_page_content 测试 --------

    #[test]
    fn test_get_page_content_home() {
        assert!(get_page_content(PageId::Home).is_none());
    }

    #[test]
    fn test_get_page_content_main_menu() {
        let content = get_page_content(PageId::MainMenu);
        assert!(content.is_some());
        assert_eq!(content.unwrap().title, "Menu");
    }

    #[test]
    fn test_get_page_content_mode_select() {
        let content = get_page_content(PageId::ModeSelect);
        assert!(content.is_some());
        assert_eq!(content.unwrap().title, "Mode");
    }

    #[test]
    fn test_get_page_content_ble_settings() {
        let content = get_page_content(PageId::BleSettings);
        assert!(content.is_some());
        assert_eq!(content.unwrap().title, "Bluetooth");
    }

    #[test]
    fn test_get_page_content_about() {
        let content = get_page_content(PageId::About);
        assert!(content.is_some());
        assert_eq!(content.unwrap().title, "About");
    }

    // -------- 模式转换测试 --------

    #[test]
    fn test_index_to_mode() {
        assert_eq!(index_to_mode(0), KeyboardMode::Media);
        assert_eq!(index_to_mode(1), KeyboardMode::Excel);
        assert_eq!(index_to_mode(2), KeyboardMode::Claude);
    }

    #[test]
    fn test_index_to_mode_out_of_range() {
        // 超出范围应该返回默认值 Media
        assert_eq!(index_to_mode(3), KeyboardMode::Media);
        assert_eq!(index_to_mode(255), KeyboardMode::Media);
    }

    #[test]
    fn test_mode_to_index() {
        assert_eq!(mode_to_index(KeyboardMode::Media), 0);
        assert_eq!(mode_to_index(KeyboardMode::Excel), 1);
        assert_eq!(mode_to_index(KeyboardMode::Claude), 2);
    }

    #[test]
    fn test_mode_index_roundtrip() {
        // 确保 mode -> index -> mode 往返转换正确
        for mode in KeyboardMode::ALL {
            let index = mode_to_index(mode);
            let back = index_to_mode(index);
            assert_eq!(mode, back);
        }
    }

    // -------- 一致性测试 --------

    #[test]
    fn test_mode_select_items_match_keyboard_modes() {
        // 确保模式选择菜单项数量与 KeyboardMode::ALL 一致
        assert_eq!(MODE_SELECT_ITEMS.len(), KeyboardMode::ALL.len());
    }

    #[test]
    fn test_main_menu_navigation_targets_exist() {
        // 确保主菜单中的所有导航目标都能获取到页面内容
        for item in MAIN_MENU_ITEMS {
            if let Some(target) = item.target {
                assert!(
                    get_page_content(target).is_some(),
                    "Target {:?} should have page content",
                    target
                );
            }
        }
    }
}
