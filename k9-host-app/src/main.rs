// INPUT:  gpui, env_logger, app_state, bridge, test_bridge, test_view, test_state, providers module
// OUTPUT: K9-Pad GPUI 桌面管理应用（窗口创建 + tokio 桥接 + 测试控制台 + 状态驱动 UI）
// POS:    桌面应用入口 — 初始化 GPUI 窗口，启动 tokio 线程，桥接 BLE 状态到 UI，提供测试控制台页面
mod app_state;
mod bridge;
pub mod providers;
mod test_bridge;
mod test_state;
mod test_view;

use std::sync::mpsc;
use std::time::Duration;

use app_state::{AppState, ConnectionStatus, Page};
use gpui::{
    div, px, rgb, size, App, AppContext, Application, BorrowAppContext, Bounds, Context,
    InteractiveElement, IntoElement, ParentElement, Render, SharedString, StyleRefinement, Styled,
    Subscription, TitlebarOptions, Window, WindowBounds, WindowOptions,
};
use test_state::TestEvent;
use test_view::{TestCommandSender, TestView};

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

    fn render_home(&self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
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
            .flex_col()
            .items_center()
            .justify_center()
            .gap(px(20.0))
            .bg(rgb(0x1e1e2e))
            .text_color(rgb(0xcdd6f4))
            .child(status_text)
            .child(
                div()
                    .px(px(16.0))
                    .py(px(8.0))
                    .bg(rgb(0x45475a))
                    .text_color(rgb(0x89b4fa))
                    .rounded(px(6.0))
                    .cursor_pointer()
                    .hover(|s: StyleRefinement| s.bg(rgb(0x585b70)))
                    .child(SharedString::from("Test Console"))
                    .on_mouse_down(
                        gpui::MouseButton::Left,
                        |_ev: &gpui::MouseDownEvent, _window: &mut Window, cx: &mut App| {
                            cx.update_global::<AppState, _>(|state, _cx| {
                                state.page = Page::Test;
                            });
                        },
                    ),
            )
    }
}

impl Render for RootView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let page = cx
            .try_global::<AppState>()
            .map(|s| s.page)
            .unwrap_or(Page::Home);

        match page {
            Page::Home => self.render_home(window, cx).into_any_element(),
            Page::Test => TestView::render_page(window, cx).into_any_element(),
        }
    }
}

/// GPUI-side bridge loop for test events: drains TestEvent and updates AppState.test_state.
async fn test_bridge_loop(rx: mpsc::Receiver<TestEvent>, cx: &mut gpui::AsyncApp) {
    loop {
        cx.background_executor()
            .timer(Duration::from_millis(50))
            .await;

        loop {
            match rx.try_recv() {
                Ok(event) => {
                    let _ = cx.update_global::<AppState, _>(|state, _cx| {
                        let ts = &mut state.test_state;
                        match event {
                            TestEvent::Connected => {
                                ts.connection = ConnectionStatus::Connected;
                            }
                            TestEvent::Disconnected => {
                                ts.connection = ConnectionStatus::Disconnected;
                                ts.device_caps = None;
                                ts.pad_config = None;
                            }
                            TestEvent::Error(msg) => {
                                ts.add_log(msg.clone(), true);
                                if matches!(ts.connection, ConnectionStatus::Connecting) {
                                    ts.connection = ConnectionStatus::Error(msg);
                                }
                            }
                            TestEvent::Log(msg) => {
                                ts.add_log(msg, false);
                            }
                            TestEvent::DeviceCaps(caps) => {
                                ts.device_caps = Some(caps);
                            }
                            TestEvent::PadConfig(config) => {
                                ts.pad_config = Some(config);
                            }
                        }
                    });
                }
                Err(mpsc::TryRecvError::Empty) => break,
                Err(mpsc::TryRecvError::Disconnected) => return,
            }
        }
    }
}

fn main() {
    env_logger::init();

    Application::new().run(|app| {
        app.set_global(AppState::default());

        // Start the main provider bridge (BLE auto-connect + providers)
        let (event_rx, _handle) = bridge::start_tokio_thread();
        app.spawn(async move |cx| bridge::bridge_loop(event_rx, cx).await)
            .detach();

        // Start the test bridge (manual BLE/USB connect + test commands)
        let (cmd_tx, test_event_rx, _test_handle) = test_bridge::start_test_thread();
        app.set_global(TestCommandSender(cmd_tx));
        app.spawn(async move |cx| test_bridge_loop(test_event_rx, cx).await)
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
