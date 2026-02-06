// menu/renderer.rs - UI 渲染器
//
// 参考 WouoUI 设计：
// - 列表菜单渲染
// - 选中项高亮（反色）
// - 滚动条
// - 标题栏

use embedded_graphics::{
    mono_font::{ascii::FONT_6X10, MonoTextStyle},
    pixelcolor::BinaryColor,
    prelude::*,
    primitives::{PrimitiveStyle, Rectangle},
    text::{Alignment, Text},
};

use super::page::{get_page_content, PageContent, MenuItem};
use super::state::{MenuState, PageId};
use crate::mode::KeyboardMode;

/// 显示常量
const DISPLAY_WIDTH: i32 = 128;
const DISPLAY_HEIGHT: i32 = 64;

/// 布局常量
const TITLE_HEIGHT: i32 = 14;           // 标题栏高度
const ITEM_HEIGHT: i32 = 12;            // 菜单项高度
const SCROLLBAR_WIDTH: i32 = 2;         // 滚动条宽度
const CONTENT_START_Y: i32 = TITLE_HEIGHT + 2;
const VISIBLE_ITEMS: i32 = (DISPLAY_HEIGHT - CONTENT_START_Y) / ITEM_HEIGHT;

/// 菜单渲染器
pub struct MenuRenderer;

impl MenuRenderer {
    /// 渲染菜单界面
    pub fn render<D>(display: &mut D, state: &MenuState, current_mode: KeyboardMode)
    where
        D: DrawTarget<Color = BinaryColor>,
    {
        let _ = display.clear(BinaryColor::Off);

        match state.current_page {
            PageId::Home => {
                // 首页不由菜单渲染器处理
            }
            _ => {
                if let Some(content) = get_page_content(state.current_page) {
                    Self::render_page(display, content, state, current_mode);
                }
            }
        }
    }

    /// 渲染页面
    fn render_page<D>(
        display: &mut D,
        content: &PageContent,
        state: &MenuState,
        current_mode: KeyboardMode,
    )
    where
        D: DrawTarget<Color = BinaryColor>,
    {
        // 绘制标题栏
        Self::draw_title_bar(display, content.title);

        // 绘制菜单列表
        Self::draw_menu_list(display, content, state, current_mode);

        // 绘制滚动条（如果需要）
        let total_items = content.items.len() as i32;
        if total_items > VISIBLE_ITEMS {
            Self::draw_scrollbar(display, state.selected_index as i32, total_items);
        }
    }

    /// 绘制标题栏
    fn draw_title_bar<D>(display: &mut D, title: &str)
    where
        D: DrawTarget<Color = BinaryColor>,
    {
        // 标题栏背景（反色）
        let _ = Rectangle::new(Point::new(0, 0), Size::new(DISPLAY_WIDTH as u32, TITLE_HEIGHT as u32))
            .into_styled(PrimitiveStyle::with_fill(BinaryColor::On))
            .draw(display);

        // 标题文字（黑色，因为背景是白色）
        let style = MonoTextStyle::new(&FONT_6X10, BinaryColor::Off);
        let _ = Text::with_alignment(
            title,
            Point::new(DISPLAY_WIDTH / 2, TITLE_HEIGHT - 3),
            style,
            Alignment::Center,
        )
        .draw(display);
    }

    /// 绘制菜单列表
    fn draw_menu_list<D>(
        display: &mut D,
        content: &PageContent,
        state: &MenuState,
        current_mode: KeyboardMode,
    )
    where
        D: DrawTarget<Color = BinaryColor>,
    {
        let selected = state.selected_index as i32;
        let total_items = content.items.len() as i32;

        // 计算滚动偏移，确保选中项可见
        let scroll_offset = Self::calculate_scroll_offset(selected, total_items);

        let style_normal = MonoTextStyle::new(&FONT_6X10, BinaryColor::On);
        let style_selected = MonoTextStyle::new(&FONT_6X10, BinaryColor::Off);

        for (i, item) in content.items.iter().enumerate() {
            let item_index = i as i32;
            let visible_index = item_index - scroll_offset;

            // 只绘制可见项
            if visible_index < 0 || visible_index >= VISIBLE_ITEMS {
                continue;
            }

            let y = CONTENT_START_Y + visible_index * ITEM_HEIGHT;
            let is_selected = item_index == selected && item.selectable;

            // 选中项背景
            if is_selected {
                let _ = Rectangle::new(
                    Point::new(0, y),
                    Size::new((DISPLAY_WIDTH - SCROLLBAR_WIDTH - 1) as u32, ITEM_HEIGHT as u32),
                )
                .into_styled(PrimitiveStyle::with_fill(BinaryColor::On))
                .draw(display);
            }

            // 文本
            let style = if is_selected { style_selected } else { style_normal };
            let label = Self::get_item_label(item, state.current_page, i as u8, current_mode);

            let _ = Text::new(label, Point::new(4, y + ITEM_HEIGHT - 3), style).draw(display);

            // 如果是当前模式，显示勾选标记
            if state.current_page == PageId::ModeSelect {
                let item_mode = super::page::index_to_mode(i as u8);
                if item_mode == current_mode {
                    let check_style = if is_selected { style_selected } else { style_normal };
                    let _ = Text::new(
                        "*",
                        Point::new(DISPLAY_WIDTH - SCROLLBAR_WIDTH - 12, y + ITEM_HEIGHT - 3),
                        check_style,
                    )
                    .draw(display);
                }
            }
        }
    }

    /// 获取菜单项显示文本（可能需要动态内容）
    fn get_item_label(
        item: &MenuItem,
        page: PageId,
        index: u8,
        _current_mode: KeyboardMode,
    ) -> &'static str {
        // 大部分情况直接返回静态标签
        // 未来可以根据页面和索引返回动态内容
        match page {
            PageId::About if index == 1 => "FW: v0.2.0",
            _ => item.label,
        }
    }

    /// 计算滚动偏移，确保选中项始终可见
    fn calculate_scroll_offset(selected: i32, total: i32) -> i32 {
        if total <= VISIBLE_ITEMS {
            return 0;
        }

        // 保持选中项在可视区域中间偏上
        let half_visible = VISIBLE_ITEMS / 2;
        let offset = (selected - half_visible).max(0);
        let max_offset = total - VISIBLE_ITEMS;

        offset.min(max_offset)
    }

    /// 绘制滚动条
    fn draw_scrollbar<D>(display: &mut D, selected: i32, total: i32)
    where
        D: DrawTarget<Color = BinaryColor>,
    {
        let scrollbar_height = DISPLAY_HEIGHT - CONTENT_START_Y;
        let thumb_height = (scrollbar_height * VISIBLE_ITEMS / total).max(4);
        let thumb_range = scrollbar_height - thumb_height;
        let thumb_pos = if total > 1 {
            CONTENT_START_Y + (thumb_range * selected / (total - 1))
        } else {
            CONTENT_START_Y
        };

        // 滚动条轨道
        let _ = Rectangle::new(
            Point::new(DISPLAY_WIDTH - SCROLLBAR_WIDTH, CONTENT_START_Y),
            Size::new(SCROLLBAR_WIDTH as u32, scrollbar_height as u32),
        )
        .into_styled(PrimitiveStyle::with_stroke(BinaryColor::On, 1))
        .draw(display);

        // 滚动条滑块
        let _ = Rectangle::new(
            Point::new(DISPLAY_WIDTH - SCROLLBAR_WIDTH, thumb_pos),
            Size::new(SCROLLBAR_WIDTH as u32, thumb_height as u32),
        )
        .into_styled(PrimitiveStyle::with_fill(BinaryColor::On))
        .draw(display);
    }
}

/// 绘制简单的确认对话框
pub fn draw_confirm_dialog<D>(display: &mut D, message: &str, selected: bool)
where
    D: DrawTarget<Color = BinaryColor>,
{
    // 对话框背景
    let dialog_width = 100;
    let dialog_height = 40;
    let x = (DISPLAY_WIDTH - dialog_width) / 2;
    let y = (DISPLAY_HEIGHT - dialog_height) / 2;

    // 清除对话框区域
    let _ = Rectangle::new(
        Point::new(x, y),
        Size::new(dialog_width as u32, dialog_height as u32),
    )
    .into_styled(PrimitiveStyle::with_fill(BinaryColor::Off))
    .draw(display);

    // 边框
    let _ = Rectangle::new(
        Point::new(x, y),
        Size::new(dialog_width as u32, dialog_height as u32),
    )
    .into_styled(PrimitiveStyle::with_stroke(BinaryColor::On, 1))
    .draw(display);

    let style = MonoTextStyle::new(&FONT_6X10, BinaryColor::On);

    // 消息
    let _ = Text::with_alignment(
        message,
        Point::new(DISPLAY_WIDTH / 2, y + 15),
        style,
        Alignment::Center,
    )
    .draw(display);

    // 按钮
    let btn_y = y + dialog_height - 12;
    let cancel_style = if !selected {
        MonoTextStyle::new(&FONT_6X10, BinaryColor::Off)
    } else {
        style
    };
    let ok_style = if selected {
        MonoTextStyle::new(&FONT_6X10, BinaryColor::Off)
    } else {
        style
    };

    // Cancel 按钮
    if !selected {
        let _ = Rectangle::new(Point::new(x + 5, btn_y - 8), Size::new(40, 10))
            .into_styled(PrimitiveStyle::with_fill(BinaryColor::On))
            .draw(display);
    }
    let _ = Text::new("Cancel", Point::new(x + 8, btn_y), cancel_style).draw(display);

    // OK 按钮
    if selected {
        let _ = Rectangle::new(Point::new(x + dialog_width - 35, btn_y - 8), Size::new(30, 10))
            .into_styled(PrimitiveStyle::with_fill(BinaryColor::On))
            .draw(display);
    }
    let _ = Text::new("OK", Point::new(x + dialog_width - 32, btn_y), ok_style).draw(display);
}
