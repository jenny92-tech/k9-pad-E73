// INPUT:  gpui, app_state (AppState, Page, ConnectionStatus), test_state (TestCommand, TransportType)
// OUTPUT: TestView, TestCommandSender (Global) — 测试控制台 GPUI 页面
// POS:    测试页面 UI — 提供手动 BLE/USB 连接、消息发送、日志查看的交互界面

use std::sync::mpsc;

use gpui::{
    div, px, rgb, AnyElement, App, BorrowAppContext, InteractiveElement, IntoElement,
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
                    .child(cmd_button(
                        "Connect",
                        btn_bg,
                        btn_active,
                        green,
                        move |cx| {
                            let tt = cx
                                .try_global::<AppState>()
                                .map(|s| s.test_state.transport_type)
                                .unwrap_or(TransportType::Ble);
                            send_cmd(cx, TestCommand::Connect(tt));
                            cx.update_global::<AppState, _>(|state, _cx| {
                                state.test_state.connection = ConnectionStatus::Connecting;
                            });
                        },
                    ))
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
                // Notch panel flow section
                div()
                    .flex()
                    .flex_col()
                    .gap(px(8.0))
                    .child(section_header("Notch Flow", subtext))
                    .child(
                        div()
                            .flex()
                            .flex_wrap()
                            .gap(px(6.0))
                            .children(notch_flow_buttons(btn_bg, btn_active, accent)),
                    ),
            )
            .child(
                // Display components section（组件网格测试）
                div()
                    .flex()
                    .flex_col()
                    .gap(px(8.0))
                    .child(section_header("Display Components", subtext))
                    .child(
                        div()
                            .flex()
                            .flex_wrap()
                            .gap(px(6.0))
                            .children(component_buttons(btn_bg, btn_active, accent)),
                    ),
            )
            .child(
                // Dialog section（设备弹窗选择器测试）
                div()
                    .flex()
                    .flex_col()
                    .gap(px(8.0))
                    .child(section_header("Dialog Test", subtext))
                    .child(
                        div()
                            .flex()
                            .flex_wrap()
                            .gap(px(6.0))
                            .children(dialog_buttons(btn_bg, btn_active, accent)),
                    ),
            )
            .child(
                // Notch settings section（实时生效 + 持久化到 ~/.k9pad/config.json）
                notch_settings_section(cx, subtext, btn_bg, btn_active, accent),
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
                            .child(div().flex().flex_col().gap(px(2.0)).children(
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
                            )),
                    ),
            )
    }
}

fn section_header(label: &str, color: u32) -> impl IntoElement + 'static {
    let text: SharedString =
        format!("\u{2500}\u{2500}\u{2500} {label} \u{2500}\u{2500}\u{2500}").into();
    div().text_size(px(12.0)).text_color(rgb(color)).child(text)
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
    ]
}

/// 组件布局/值测试按钮（新架构：设备本地渲染组件网格）
fn component_buttons(btn_bg: u32, btn_active: u32, accent: u32) -> Vec<AnyElement> {
    vec![
        cmd_button("布局 2×2", btn_bg, btn_active, accent, |cx| {
            send_cmd(cx, TestCommand::CompLayout);
        })
        .into_any_element(),
        cmd_button("Time 17:52", btn_bg, btn_active, accent, |cx| {
            send_cmd(
                cx,
                TestCommand::CompSetText {
                    id: 1,
                    text: "17:52".into(),
                    wake: false,
                },
            );
        })
        .into_any_element(),
        cmd_button("Vol 75%", btn_bg, btn_active, accent, |cx| {
            send_cmd(
                cx,
                TestCommand::CompSetU8 {
                    id: 2,
                    value: 75,
                    wake: false,
                },
            );
        })
        .into_any_element(),
        cmd_button("Subs 12345", btn_bg, btn_active, accent, |cx| {
            send_cmd(
                cx,
                TestCommand::CompSetNumeric {
                    id: 3,
                    value: 12345,
                    wake: false,
                },
            );
        })
        .into_any_element(),
        cmd_button("AI 60%", btn_bg, btn_active, accent, |cx| {
            send_cmd(
                cx,
                TestCommand::CompSetPercent {
                    id: 4,
                    value: 60,
                    wake: false,
                },
            );
        })
        .into_any_element(),
        cmd_button("Checkbox ✓", btn_bg, btn_active, accent, |cx| {
            send_cmd(
                cx,
                TestCommand::CompSetCheckbox {
                    id: 5,
                    on: true,
                    wake: false,
                },
            );
        })
        .into_any_element(),
        cmd_button("通知(wake)", btn_bg, btn_active, accent, |cx| {
            send_cmd(
                cx,
                TestCommand::CompSetText {
                    id: 1,
                    text: "新通知!".into(),
                    wake: true,
                },
            );
        })
        .into_any_element(),
    ]
}

/// 弹窗测试按钮（授权 + AI 多选；设备滚轮选择 + Select/Back 确认）
fn dialog_buttons(btn_bg: u32, btn_active: u32, accent: u32) -> Vec<AnyElement> {
    vec![
        cmd_button("授权弹窗", btn_bg, btn_active, accent, |cx| {
            send_cmd(cx, TestCommand::ShowConfirmDialog);
        })
        .into_any_element(),
        cmd_button("AI 多选弹窗", btn_bg, btn_active, accent, |cx| {
            send_cmd(cx, TestCommand::ShowChoiceDialog);
        })
        .into_any_element(),
    ]
}

/// 刘海面板状态自测：Idle / Working / 当前工具 / Allow-Deny / Options / End。
/// 所有按钮都经真实 hook socket 注入，不绕过 reducer 或 pending oneshot。
fn notch_flow_buttons(btn_bg: u32, btn_active: u32, accent: u32) -> Vec<AnyElement> {
    const IDLE: &str = r#"{"session_id":"ui-test-1","hook_event_name":"SessionStart","cwd":"/tmp","_source":"claude"}"#;
    const WORKING: &str = r#"{"session_id":"ui-test-1","hook_event_name":"UserPromptSubmit","prompt":"refine the notch panel","cwd":"/tmp","_source":"claude"}"#;
    const TOOL: &str = r#"{"session_id":"ui-test-1","hook_event_name":"PreToolUse","tool_name":"Bash","tool_input":{"command":"cargo test"},"cwd":"/tmp","_source":"claude"}"#;
    const PERMISSION: &str = r#"{"session_id":"ui-test-1","hook_event_name":"PermissionRequest","tool_name":"Bash","tool_input":{"command":"rm -rf /tmp/k9-test"},"cwd":"/tmp","_source":"claude"}"#;
    const OPTIONS: &str = r#"{"session_id":"ui-test-1","hook_event_name":"PermissionRequest","tool_name":"AskUserQuestion","tool_input":{"questions":[{"question":"Choose a release channel","options":[{"label":"Stable","description":"Recommended"},{"label":"Beta"},{"label":"Nightly"}]},{"question":"Run extra checks?","multiSelect":true,"options":[{"label":"Tests"},{"label":"Clippy"},{"label":"Audit"}]}]},"cwd":"/tmp","_source":"claude"}"#;
    const SESSION_END: &str =
        r#"{"session_id":"ui-test-1","hook_event_name":"SessionEnd","_source":"claude"}"#;

    vec![
        cmd_button("Idle", btn_bg, btn_active, accent, |cx| {
            send_cmd(
                cx,
                TestCommand::InjectHook {
                    json: IDLE.into(),
                    blocking: false,
                },
            );
        })
        .into_any_element(),
        cmd_button("Working", btn_bg, btn_active, accent, |cx| {
            send_cmd(
                cx,
                TestCommand::InjectHook {
                    json: WORKING.into(),
                    blocking: false,
                },
            );
        })
        .into_any_element(),
        cmd_button("Tool: Bash", btn_bg, btn_active, accent, |cx| {
            send_cmd(
                cx,
                TestCommand::InjectHook {
                    json: TOOL.into(),
                    blocking: false,
                },
            );
        })
        .into_any_element(),
        cmd_button("Allow / Deny", btn_bg, btn_active, accent, |cx| {
            send_cmd(
                cx,
                TestCommand::InjectHook {
                    json: PERMISSION.into(),
                    blocking: true,
                },
            );
        })
        .into_any_element(),
        cmd_button("Options", btn_bg, btn_active, accent, |cx| {
            send_cmd(
                cx,
                TestCommand::InjectHook {
                    json: OPTIONS.into(),
                    blocking: true,
                },
            );
        })
        .into_any_element(),
        cmd_button("End", btn_bg, btn_active, accent, |cx| {
            send_cmd(
                cx,
                TestCommand::InjectHook {
                    json: SESSION_END.into(),
                    blocking: false,
                },
            );
        })
        .into_any_element(),
    ]
}

/// 刘海设置区块：端部样式切换 + 响应式宽度基线微调（实时生效并持久化）
fn notch_settings_section(
    cx: &App,
    subtext: u32,
    btn_bg: u32,
    btn_active: u32,
    accent: u32,
) -> impl IntoElement {
    use crate::notch_shape::EndStyle;

    let (style, factor) = cx
        .try_global::<AppState>()
        .map(|s| (s.notch_style, s.widen_factor))
        .unwrap_or((EndStyle::Notch, 1.8));
    let baseline_scale = factor / crate::notch_config::DEFAULT_WIDEN_FACTOR;

    let style_btn = move |label: &str, value: EndStyle| {
        let selected = style == value;
        let bg = if selected { accent } else { btn_bg };
        let fg = if selected { 0x1e1e2e } else { 0xcdd6f4 };
        let label_owned: SharedString = label.to_owned().into();
        div()
            .px(px(12.0))
            .py(px(6.0))
            .bg(rgb(bg))
            .text_color(rgb(fg))
            .rounded(px(6.0))
            .cursor_pointer()
            .hover(move |s: StyleRefinement| s.bg(rgb(if selected { accent } else { btn_active })))
            .child(label_owned)
            .on_mouse_down(
                gpui::MouseButton::Left,
                move |_ev: &gpui::MouseDownEvent, _window: &mut Window, cx: &mut App| {
                    cx.update_global::<AppState, _>(|state, _cx| {
                        state.notch_style = value;
                        crate::notch_config::save(
                            value,
                            state.widen_factor,
                            state.notch_top_r,
                            state.notch_bot_r,
                        );
                    });
                },
            )
    };

    let step_btn = |label: &str, delta: f32| {
        let label_owned: SharedString = label.to_owned().into();
        div()
            .px(px(12.0))
            .py(px(6.0))
            .bg(rgb(btn_bg))
            .text_color(rgb(0xcdd6f4))
            .rounded(px(6.0))
            .cursor_pointer()
            .hover(move |s: StyleRefinement| s.bg(rgb(btn_active)))
            .child(label_owned)
            .on_mouse_down(
                gpui::MouseButton::Left,
                move |_ev: &gpui::MouseDownEvent, _window: &mut Window, cx: &mut App| {
                    cx.update_global::<AppState, _>(|state, _cx| {
                        state.widen_factor = (state.widen_factor + delta).clamp(1.0, 3.0);
                        crate::notch_config::save(
                            state.notch_style,
                            state.widen_factor,
                            state.notch_top_r,
                            state.notch_bot_r,
                        );
                    });
                },
            )
    };

    div()
        .flex()
        .flex_col()
        .gap(px(8.0))
        .child(section_header("Notch Settings", subtext))
        .child(
            div()
                .flex()
                .items_center()
                .flex_wrap()
                .gap(px(6.0))
                .child(
                    div()
                        .text_color(rgb(subtext))
                        .child(SharedString::from("Style:")),
                )
                .child(style_btn("Notch (上直下圆)", EndStyle::Notch))
                .child(style_btn("Capsule (胶囊)", EndStyle::Capsule))
                .child(div().w(px(12.0)))
                .child(
                    div()
                        .text_color(rgb(subtext))
                        .child(SharedString::from("Width tuning:")),
                )
                .child(step_btn("-", -0.2))
                .child(
                    div()
                        .w(px(48.0))
                        .text_color(rgb(accent))
                        .child(SharedString::from(format!("{baseline_scale:.2}x"))),
                )
                .child(step_btn("+", 0.2)),
        )
}

fn send_cmd(cx: &App, cmd: TestCommand) {
    if let Some(sender) = cx.try_global::<TestCommandSender>() {
        let _ = sender.0.send(cmd);
    }
}
