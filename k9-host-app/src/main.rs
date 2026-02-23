// INPUT:  gpui, k9-host-lib, shared-datachannel-proto
// OUTPUT: K9-Pad 桌面管理应用
// POS:    GPUI 桌面应用入口，替代原 k9-host-cli
pub mod providers;

use gpui::{
    div, px, size, AppContext, Application, Bounds, Context, IntoElement, ParentElement, Render,
    SharedString, Styled, TitlebarOptions, Window, WindowBounds, WindowOptions,
};

struct RootView;

impl Render for RootView {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .size_full()
            .flex()
            .items_center()
            .justify_center()
            .bg(gpui::rgb(0x1e1e2e))
            .text_color(gpui::rgb(0xcdd6f4))
            .child("K9-Pad Host — connecting...")
    }
}

fn main() {
    env_logger::init();

    Application::new().run(|app| {
        let options = WindowOptions {
            titlebar: Some(TitlebarOptions {
                title: Some(SharedString::from("K9-Pad Manager")),
                ..Default::default()
            }),
            window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
                None,
                size(px(800.0), px(600.0)),
                app,
            ))),
            focus: true,
            show: true,
            ..Default::default()
        };
        app.open_window(options, |_window, app| app.new(|_cx| RootView))
            .unwrap();
    });
}
