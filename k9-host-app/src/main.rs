// INPUT:  gpui, env_logger, app_state, bridge (含 HostCommand), hook_server, session, notch_panel, notch_native, notch_shape, spring, test_bridge, test_view, test_state, providers module
// OUTPUT: K9-Pad GPUI 桌面管理应用（窗口创建 + tokio 桥接 + 测试控制台 + 弹窗测试入口 + 刘海变形面板 + 状态驱动 UI）
// POS:    桌面应用入口 — 初始化 GPUI 窗口，启动 tokio 线程（BLE + hook socket server），桥接 BLE/hook 状态到 UI，提供测试控制台页面与设备弹窗最小触发入口
mod app_state;
mod bridge;
mod hook_server;
mod notch_config;
#[cfg(target_os = "macos")]
mod notch_native;
mod notch_panel;
mod notch_shape;
pub mod providers;
mod session;
mod spring;
mod test_bridge;
mod test_state;
mod test_view;

use std::sync::mpsc;
use std::time::Duration;

use app_state::{AppState, ConnectionStatus, Page};
use bridge::HostCommand;
use gpui::{
    div, px, rgb, size, App, AppContext, Application, BorrowAppContext, Bounds, Context,
    InteractiveElement, IntoElement, ParentElement, Render, SharedString, StyleRefinement, Styled,
    Subscription, TitlebarOptions, Window, WindowBounds, WindowKind, WindowOptions,
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
        let (status_text, dialog_enabled, dialog_hint, dialog_result_text): (
            SharedString,
            bool,
            SharedString,
            SharedString,
        ) = match cx.try_global::<AppState>() {
            Some(state) => {
                let status: SharedString = match &state.connection {
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
                };

                let enabled = matches!(state.connection, ConnectionStatus::Connected);
                let hint: SharedString = if !matches!(state.connection, ConnectionStatus::Connected)
                {
                    "Dialog unavailable: not connected".into()
                } else {
                    "Dialog unavailable: device protocol < v2".into()
                };
                let result: SharedString = match &state.last_dialog_result {
                    Some((id, r)) => format!(
                        "Dialog #{id}: {r:?} ({} received)",
                        state.dialog_result_count
                    )
                    .into(),
                    None => "Dialog: no result yet".into(),
                };
                (status, enabled, hint, result)
            }
            None => ("Initializing...".into(), false, "".into(), "".into()),
        };

        let dialog_button = div()
            .px(px(16.0))
            .py(px(8.0))
            .rounded(px(6.0))
            .child(SharedString::from("Send Test Dialog"));
        let dialog_button = if dialog_enabled {
            dialog_button
                .bg(rgb(0x45475a))
                .text_color(rgb(0x89b4fa))
                .cursor_pointer()
                .hover(|s: StyleRefinement| s.bg(rgb(0x585b70)))
                .on_mouse_down(
                    gpui::MouseButton::Left,
                    |_ev: &gpui::MouseDownEvent, _window: &mut Window, cx: &mut App| {
                        cx.update_global::<AppState, _>(|state, _cx| {
                            if let Some(tx) = &state.host_command_tx {
                                let _ = tx.send(HostCommand::ShowDialog {
                                    text: "Confirm on device?".into(),
                                });
                            }
                        });
                    },
                )
        } else {
            dialog_button.bg(rgb(0x313244)).text_color(rgb(0x6c7086))
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
            .child(dialog_button)
            .child(
                div()
                    .text_color(rgb(0xa6adc8))
                    .child(if dialog_enabled {
                        dialog_result_text
                    } else {
                        dialog_hint
                    }),
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

    // macOS 常驻面板防杀：系统 Automatic Termination 会把「无可见窗口的闲置后台应用」
    // 静默 terminate（exit 0 无日志，约 20 分钟触发），常驻刘海面板必须禁用。
    #[cfg(target_os = "macos")]
    {
        use objc2_foundation::{NSProcessInfo, NSString};
        let pi = NSProcessInfo::processInfo();
        pi.disableAutomaticTermination(&NSString::from_str(
            "K9-Pad runs a persistent notch approval panel",
        ));
        pi.disableSuddenTermination();
    }

    Application::new().run(|app| {
        let cfg = notch_config::load();
        app.set_global(AppState {
            notch_style: cfg.style,
            widen_factor: cfg.widen_factor,
            notch_top_r: cfg.top_r,
            notch_bot_r: cfg.bot_r,
            ..Default::default()
        });

        // Start the main provider bridge (BLE auto-connect + providers)
        let (event_rx, cmd_tx, _handle) = bridge::start_tokio_thread();
        app.update_global::<AppState, _>(|state, _cx| {
            state.host_command_tx = Some(cmd_tx);
        });
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
            // GPUI 0.2 的 Normal 窗口在 macOS 26 上创建后 WindowServer 侧尺寸损坏
            // （实测被钳成 79x109 迷你窗）；PopUp（NSPanel）类型窗口无此问题
            kind: WindowKind::PopUp,
            window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
                None,
                size(px(800.0), px(600.0)),
                app,
            ))),
            // 测试控制台内容无滚动兜底，窗口被拉太小会压成一团不可用——锁最小尺寸
            window_min_size: Some(size(px(720.0), px(520.0))),
            focus: true,
            show: true,
            ..Default::default()
        };
        app.open_window(options, |window, cx| {
            #[cfg(target_os = "macos")]
            fix_window_frame_after_launch(window);
            cx.new(|cx| RootView::new(cx))
        })
        .unwrap();

        // 刘海变形面板：单窗口三态状态机（Hidden/Widened/Dropped），由 AppState 驱动
        notch_panel::open_notch_panel_window(app);
    });
}

/// macOS 修正：GPUI 0.2 创建窗口时 y 轴翻转漏减窗口高度，窗口落点越出屏幕顶，
/// AppKit 钳制后 WindowServer 侧显示尺寸损坏（实测被钳成 79x102 的迷你窗）。
/// 创建后在主队列延迟 re-assert 一次正确 frame 即可恢复（刘海窗口由
/// notch_native 的 pin 逻辑天然免疫——它总是自己 setFrame）。
#[cfg(target_os = "macos")]
fn fix_window_frame_after_launch(window: &mut gpui::Window) {
    use objc2_foundation::{NSPoint, NSRect, NSSize};
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};

    struct FrameJob {
        win: usize,
        x: f64,
        y: f64,
        w: f64,
        h: f64,
    }
    extern "C" fn run(ctx: *mut core::ffi::c_void) {
        // SAFETY: ctx 来自下方 Box::into_raw，主队列恰好消费一次并释放
        let job = unsafe { Box::from_raw(ctx as *mut FrameJob) };
        let w = unsafe { &*(job.win as *const objc2_app_kit::NSWindow) };
        let f = NSRect::new(NSPoint::new(job.x, job.y), NSSize::new(job.w, job.h));
        // SAFETY: 主队列执行；窗口属于本应用生命周期
        let () = unsafe { objc2::msg_send![w, setFrame: f, display: true] };
        // 刷新 WindowServer 侧状态：orderOut/orderFront 强制重挂窗口
        let () = unsafe { objc2::msg_send![w, orderOut: w] };
        let () = unsafe { objc2::msg_send![w, setFrame: f, display: true] };
        let () = unsafe { objc2::msg_send![w, orderFront: w] };
        log::info!("main window re-assert done: {:?}", w.frame());

        // 枚举 NSApp 全部窗口：排查是否存在第二个原生窗口
        let app = objc2_app_kit::NSApplication::sharedApplication(unsafe {
            objc2::MainThreadMarker::new_unchecked()
        });
        for (i, win) in app.windows().iter().enumerate() {
            log::info!(
                "NSApp.windows[{}]: num={} frame={:?} visible={}",
                i,
                win.windowNumber(),
                win.frame(),
                win.isVisible()
            );
        }
    }
    extern "C" {
        static _dispatch_main_q: core::ffi::c_void;
        fn dispatch_after_f(
            when: u64,
            queue: *const core::ffi::c_void,
            context: *mut core::ffi::c_void,
            work: extern "C" fn(*mut core::ffi::c_void),
        );
        fn dispatch_time(when: i64, delta: i64) -> u64;
    }

    let Ok(h) = HasWindowHandle::window_handle(window) else {
        return;
    };
    let RawWindowHandle::AppKit(kit) = h.as_raw() else {
        return;
    };
    // SAFETY: ns_view 是 GPUI 持有的有效指针，主线程读取其 window
    let view: &objc2_app_kit::NSView =
        unsafe { &*(kit.ns_view.as_ptr() as *const objc2_app_kit::NSView) };
    let Some(win) = view.window() else {
        log::warn!("fix_window_frame: no native window");
        return;
    };
    // 目标屏：优先带物理刘海的屏（用户盯着内置屏看刘海），否则窗口当前屏。
    // 旧版固定在窗口当前屏（= 主/外接屏），内置刘海屏用户会"找不到窗口"。
    let target = crate::notch_native::notched_screen_frame()
        .or_else(|| win.screen().map(|s| s.frame()));
    let Some(sf) = target else {
        log::warn!("fix_window_frame: no screen");
        return;
    };
    // 目标：800x632（600 内容 + 标题栏）居中于目标屏
    let (w, h) = (800.0, 632.0);
    let x = sf.origin.x + (sf.size.width - w) / 2.0;
    let y = sf.origin.y + (sf.size.height - h) / 2.0;
    let job = Box::into_raw(Box::new(FrameJob {
        win: objc2::rc::Retained::as_ptr(&win) as usize,
        x,
        y,
        w,
        h,
    }));
    // SAFETY: 标准 GCD 用法，1s 后主队列执行（过早会被后续的尺寸破坏覆盖）
    unsafe {
        dispatch_after_f(
            dispatch_time(0, 1_000_000_000),
            &_dispatch_main_q,
            job as *mut core::ffi::c_void,
            run,
        )
    };
}
