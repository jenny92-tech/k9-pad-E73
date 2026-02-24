// INPUT:  gpui, app_state (AppState, Page, ConnectionStatus), test_state (TestCommand, TransportType)
// OUTPUT: TestView, TestCommandSender (Global) — 测试控制台 GPUI 页面
// POS:    测试页面 UI — 提供手动 BLE/USB 连接、消息发送、日志查看的交互界面

use std::sync::mpsc;

use gpui::{
    div, px, rgb, AnyElement, App, BorrowAppContext, IntoElement, InteractiveElement,
    ParentElement, SharedString, StatefulInteractiveElement, StyleRefinement, Styled, Window,
};

use crate::app_state::{AppState, ConnectionStatus, Page};
use crate::test_state::{TestCommand, TransportType};

/// Sender for test commands, stored as a GPUI Global.
pub struct TestCommandSender(pub mpsc::Sender<TestCommand>);
impl gpui::Global for TestCommandSender {}

pub struct TestView;

impl TestView {
    /// Render the test console page content (called from RootView).
    pub fn render_page(_window: &mut Window, cx: &App) -> impl IntoElement {
        let (status_text, status_color, transport_type, device_info, logs) =
            match cx.try_global::<AppState>() {
                Some(state) => {
                    let ts = &state.test_state;
                    let (text, color) = match &ts.connection {
                        ConnectionStatus::Disconnected => ("Disconnected", 0x6c7086u32),
                        ConnectionStatus::Connecting => ("Connecting...", 0xf9e2af),
                        ConnectionStatus::Connected => ("Connected", 0xa6e3a1),
                        ConnectionStatus::Error(_) => ("Error", 0xf38ba8u32),
                    };

                    let device_info = ts.device_caps.as_ref().map(|caps| {
                        format!(
                            "FW {}.{}.{}  Protocol v{}",
                            caps.firmware_major,
                            caps.firmware_minor,
                            caps.firmware_patch,
                            caps.protocol_version
                        )
                    });

                    let pad_info = ts.pad_config.as_ref().map(|cfg| {
                        format!(
                            "Pad {}  Functions 0x{:04X}",
                            cfg.active_pad, cfg.enabled_functions
                        )
                    });

                    let combined = match (device_info, pad_info) {
                        (Some(d), Some(p)) => format!("{d}  |  {p}"),
                        (Some(d), None) => d,
                        (None, Some(p)) => p,
                        (None, None) => "-".to_string(),
                    };

                    let log_entries: Vec<(String, String, bool)> = ts
                        .logs
                        .iter()
                        .map(|l| (l.time.clone(), l.message.clone(), l.is_error))
                        .collect();

                    (text, color, ts.transport_type, combined, log_entries)
                }
                None => (
                    "Initializing...",
                    0x6c7086u32,
                    TransportType::Ble,
                    "-".to_string(),
                    Vec::new(),
                ),
            };

        // Colors (Catppuccin Mocha palette)
        let bg = 0x1e1e2e;
        let surface = 0x313244;
        let text_color = 0xcdd6f4;
        let subtext = 0xa6adc8;
        let btn_bg = 0x45475a;
        let btn_active = 0x585b70;
        let accent = 0x89b4fa;
        let green = 0xa6e3a1;
        let red = 0xf38ba8;

        div()
            .size_full()
            .flex()
            .flex_col()
            .bg(rgb(bg))
            .text_color(rgb(text_color))
            .p(px(20.0))
            .gap(px(16.0))
            .child(
                // Header row: Back button + title + status
                div()
                    .flex()
                    .items_center()
                    .gap(px(12.0))
                    .child(
                        div()
                            .px(px(12.0))
                            .py(px(6.0))
                            .bg(rgb(btn_bg))
                            .rounded(px(6.0))
                            .cursor_pointer()
                            .hover(|s: StyleRefinement| s.bg(rgb(btn_active)))
                            .child(SharedString::from("\u{2190} Back"))
                            .on_mouse_down(
                                gpui::MouseButton::Left,
                                |_ev: &gpui::MouseDownEvent, _window: &mut Window, cx: &mut App| {
                                    cx.update_global::<AppState, _>(|state, _cx| {
                                        state.page = Page::Home;
                                    });
                                },
                            ),
                    )
                    .child(
                        div()
                            .flex_1()
                            .text_size(px(18.0))
                            .child(SharedString::from("K9-Pad Test Console")),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(8.0))
                            .child(
                                div()
                                    .w(px(8.0))
                                    .h(px(8.0))
                                    .rounded(px(4.0))
                                    .bg(rgb(status_color)),
                            )
                            .child(SharedString::from(status_text)),
                    ),
            )
            .child(
                // Transport selection + connect/disconnect
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .child(
                        div()
                            .text_color(rgb(subtext))
                            .child(SharedString::from("Transport:")),
                    )
                    .child(transport_button(
                        "BLE",
                        TransportType::Ble,
                        transport_type,
                        accent,
                        btn_bg,
                        btn_active,
                    ))
                    .child(transport_button(
                        "USB",
                        TransportType::Usb,
                        transport_type,
                        accent,
                        btn_bg,
                        btn_active,
                    ))
                    .child(div().w(px(16.0)))
                    .child(cmd_button("Connect", btn_bg, btn_active, green, move |cx| {
                        let tt = cx
                            .try_global::<AppState>()
                            .map(|s| s.test_state.transport_type)
                            .unwrap_or(TransportType::Ble);
                        send_cmd(cx, TestCommand::Connect(tt));
                        cx.update_global::<AppState, _>(|state, _cx| {
                            state.test_state.connection = ConnectionStatus::Connecting;
                        });
                    }))
                    .child(cmd_button("Disconnect", btn_bg, btn_active, red, |cx| {
                        send_cmd(cx, TestCommand::Disconnect);
                    })),
            )
            .child(
                // Device info section
                div()
                    .flex()
                    .flex_col()
                    .gap(px(4.0))
                    .child(section_header("Device Info", subtext))
                    .child(
                        div()
                            .px(px(12.0))
                            .py(px(8.0))
                            .bg(rgb(surface))
                            .rounded(px(6.0))
                            .text_color(rgb(subtext))
                            .child(SharedString::from(device_info)),
                    ),
            )
            .child(
                // Send commands section
                div()
                    .flex()
                    .flex_col()
                    .gap(px(8.0))
                    .child(section_header("Send Commands", subtext))
                    .child(
                        div()
                            .flex()
                            .flex_wrap()
                            .gap(px(6.0))
                            .children(command_buttons(btn_bg, btn_active, accent)),
                    ),
            )
            .child(
                // Log section
                div()
                    .flex_1()
                    .flex()
                    .flex_col()
                    .gap(px(4.0))
                    .min_h(px(0.0))
                    .child(section_header("Log", subtext))
                    .child(
                        div()
                            .id("test-log-scroll")
                            .flex_1()
                            .bg(rgb(surface))
                            .rounded(px(6.0))
                            .p(px(8.0))
                            .overflow_y_scroll()
                            .child(
                                div().flex().flex_col().gap(px(2.0)).children(
                                    logs.into_iter().map(|(time, msg, is_err)| {
                                        let color = if is_err { red } else { green };
                                        div()
                                            .flex()
                                            .gap(px(8.0))
                                            .text_size(px(12.0))
                                            .child(
                                                div()
                                                    .text_color(rgb(subtext))
                                                    .child(SharedString::from(time)),
                                            )
                                            .child(
                                                div()
                                                    .text_color(rgb(color))
                                                    .child(SharedString::from(msg)),
                                            )
                                    }),
                                ),
                            ),
                    ),
            )
    }
}

fn section_header(label: &str, color: u32) -> impl IntoElement + 'static {
    let text: SharedString = format!("\u{2500}\u{2500}\u{2500} {label} \u{2500}\u{2500}\u{2500}").into();
    div()
        .text_size(px(12.0))
        .text_color(rgb(color))
        .child(text)
}

fn transport_button(
    label: &str,
    value: TransportType,
    current: TransportType,
    accent: u32,
    btn_bg: u32,
    btn_active: u32,
) -> impl IntoElement {
    let is_selected = value == current;
    let bg_color = if is_selected { accent } else { btn_bg };
    let text_c = if is_selected { 0x1e1e2eu32 } else { 0xcdd6f4 };
    let hover_bg = if is_selected { accent } else { btn_active };
    let label_owned: SharedString = label.to_owned().into();

    div()
        .px(px(12.0))
        .py(px(6.0))
        .bg(rgb(bg_color))
        .text_color(rgb(text_c))
        .rounded(px(6.0))
        .cursor_pointer()
        .hover(move |s: StyleRefinement| s.bg(rgb(hover_bg)))
        .child(label_owned)
        .on_mouse_down(
            gpui::MouseButton::Left,
            move |_ev: &gpui::MouseDownEvent, _window: &mut Window, cx: &mut App| {
                cx.update_global::<AppState, _>(|state, _cx| {
                    state.test_state.transport_type = value;
                });
            },
        )
}

fn cmd_button(
    label: &str,
    btn_bg: u32,
    btn_active: u32,
    text_color: u32,
    on_click: impl Fn(&mut App) + 'static,
) -> impl IntoElement {
    let label_owned: SharedString = label.to_owned().into();

    div()
        .px(px(12.0))
        .py(px(6.0))
        .bg(rgb(btn_bg))
        .text_color(rgb(text_color))
        .rounded(px(6.0))
        .cursor_pointer()
        .hover(move |s: StyleRefinement| s.bg(rgb(btn_active)))
        .child(label_owned)
        .on_mouse_down(
            gpui::MouseButton::Left,
            move |_ev: &gpui::MouseDownEvent, _window: &mut Window, cx: &mut App| {
                on_click(cx);
            },
        )
}

fn command_buttons(btn_bg: u32, btn_active: u32, accent: u32) -> Vec<AnyElement> {
    vec![
        cmd_button("Ping", btn_bg, btn_active, accent, |cx| {
            send_cmd(cx, TestCommand::Ping);
        })
        .into_any_element(),
        cmd_button("Get Status", btn_bg, btn_active, accent, |cx| {
            send_cmd(cx, TestCommand::GetStatus);
        })
        .into_any_element(),
        cmd_button("Get Caps", btn_bg, btn_active, accent, |cx| {
            send_cmd(cx, TestCommand::GetCapabilities);
        })
        .into_any_element(),
        cmd_button("Text \"Hello\"", btn_bg, btn_active, accent, |cx| {
            send_cmd(
                cx,
                TestCommand::PushText {
                    slot: 0,
                    text: "Hello".into(),
                },
            );
        })
        .into_any_element(),
        cmd_button("Text \"12:34\"", btn_bg, btn_active, accent, |cx| {
            send_cmd(
                cx,
                TestCommand::PushText {
                    slot: 0,
                    text: "12:34".into(),
                },
            );
        })
        .into_any_element(),
        cmd_button("Numeric 42", btn_bg, btn_active, accent, |cx| {
            send_cmd(cx, TestCommand::PushNumeric { slot: 1, value: 42 });
        })
        .into_any_element(),
        cmd_button("Numeric -999", btn_bg, btn_active, accent, |cx| {
            send_cmd(
                cx,
                TestCommand::PushNumeric {
                    slot: 1,
                    value: -999,
                },
            );
        })
        .into_any_element(),
        cmd_button("Progress 75%", btn_bg, btn_active, accent, |cx| {
            send_cmd(
                cx,
                TestCommand::PushProgress {
                    slot: 2,
                    value: 75,
                },
            );
        })
        .into_any_element(),
        cmd_button("Clear Slot 0", btn_bg, btn_active, accent, |cx| {
            send_cmd(cx, TestCommand::ClearSlot(0));
        })
        .into_any_element(),
    ]
}

fn send_cmd(cx: &App, cmd: TestCommand) {
    if let Some(sender) = cx.try_global::<TestCommandSender>() {
        let _ = sender.0.send(cmd);
    }
}
