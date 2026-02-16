// INPUT:  gpui, k9-host-lib, shared-datachannel-proto
// OUTPUT: K9-Pad 桌面管理应用
// POS:    GPUI 桌面应用入口，替代原 k9-host-cli
pub mod providers;

use gpui::{
    div, AppContext, Application, Context, IntoElement, ParentElement, Render, Styled, Window,
    WindowOptions,
};

struct RootView;

impl Render for RootView {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .size_full()
            .flex()
            .items_center()
            .justify_center()
            .child("K9-Pad Host — connecting...")
    }
}

fn main() {
    env_logger::init();

    Application::new().run(|app| {
        app.open_window(WindowOptions::default(), |_window, app| {
            app.new(|_cx| RootView)
        })
        .unwrap();
    });
}
