// INPUT:  gpui, env_logger, app_state, bridge, providers module
// OUTPUT: K9-Pad GPUI 桌面管理应用（窗口创建 + tokio 桥接 + 状态驱动 UI）
// POS:    桌面应用入口 — 初始化 GPUI 窗口，启动 tokio 线程，桥接 BLE 状态到 UI
mod app_state;
mod bridge;
pub mod providers;

use app_state::{AppState, ConnectionStatus};
use gpui::{
    div, px, size, AppContext, Application, Bounds, Context, IntoElement, ParentElement, Render,
    SharedString, Styled, Subscription, TitlebarOptions, Window, WindowBounds, WindowOptions,
};

struct RootView {
    _state_sub: Subscription,
}

impl RootView {
    fn new(cx: &mut Context<Self>) -> Self {
        let sub = cx.observe_global::<AppState>(|_this, cx| {
            cx.notify();
        });
        Self { _state_sub: sub }
    }
}

impl Render for RootView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let status_text: SharedString = match cx.try_global::<AppState>() {
            Some(state) => match &state.connection {
                ConnectionStatus::Disconnected => "Disconnected".into(),
                ConnectionStatus::Connecting => "Scanning for K9-Pad...".into(),
                ConnectionStatus::Connected => {
                    if let Some(caps) = &state.device_caps {
                        format!(
                            "Connected | FW {}.{}.{} | Protocol v{}",
                            caps.firmware_major,
                            caps.firmware_minor,
                            caps.firmware_patch,
                            caps.protocol_version
                        )
                        .into()
                    } else {
                        "Connected".into()
                    }
                }
                ConnectionStatus::Error(e) => format!("Error: {e}").into(),
            },
            None => "Initializing...".into(),
        };

        div()
            .size_full()
            .flex()
            .items_center()
            .justify_center()
            .bg(gpui::rgb(0x1e1e2e))
            .text_color(gpui::rgb(0xcdd6f4))
            .child(status_text)
    }
}

fn main() {
    env_logger::init();

    Application::new().run(|app| {
        app.set_global(AppState::default());

        let (event_rx, _handle) = bridge::start_tokio_thread();
        app.spawn(async move |cx| bridge::bridge_loop(event_rx, cx).await)
            .detach();

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
        app.open_window(options, |_window, cx| cx.new(|cx| RootView::new(cx)))
            .unwrap();
    });
}
