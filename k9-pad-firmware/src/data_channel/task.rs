// INPUT:  rmk::data_channel, super::{parse, DISPLAY_DATA, DIALOG_DATA, DIALOG_RESULT, DATA_CHANNEL_CONFIG}
// OUTPUT: run_data_channel() async task
// POS:    数据通道主任务，桥接 RMK BLE/USB 收发与显示命令/弹窗请求分发

#[cfg(not(test))]
use super::{parse, DIALOG_DATA, DIALOG_RESULT, DISPLAY_DATA, DATA_CHANNEL_CONFIG};

/// 数据通道处理主任务
///
/// 从 RMK 的 DATA_CHANNEL_RX 接收主机数据，解析协议，
/// 分发 DisplayCommand 到 DISPLAY_DATA channel、DialogRequest 到 DIALOG_DATA channel。
/// 同时监听菜单配置变化（CONFIG_CHANGED）与弹窗结果（DIALOG_RESULT），
/// 分别组包发送到 DATA_CHANNEL_TX 回传主机。
#[cfg(not(test))]
pub async fn run_data_channel() -> ! {
    use k9_datachannel_proto::{build_config_changed, build_dialog_result, DialogResultCode, PadConfig};
    use rmk::data_channel::{DATA_CHANNEL_RX, DATA_CHANNEL_TX};
    use rmk::embassy_futures::select::{select3, Either3};

    defmt::info!("Data channel task started");

    // 配置变化监听
    let mut config_rx = DATA_CHANNEL_CONFIG
        .receiver()
        .expect("DATA_CHANNEL_CONFIG: no receiver slot available (max 2)");

    // Track latest config so handle_control_packet can reply with real data
    let mut current_config = PadConfig::default();

    loop {
        // 同时等待：主机数据 / 配置变化 / 弹窗结果
        match select3(
            DATA_CHANNEL_RX.receive(),
            config_rx.changed(),
            DIALOG_RESULT.receive(),
        )
        .await
        {
            // 收到主机数据
            Either3::First(rx_buf) => {
                // 尝试解析为显示命令（旧 slot 命令）
                if let Some(cmd) = parse::parse_display_packet(&rx_buf) {
                    let _ = DISPLAY_DATA.try_send(cmd);
                }

                // 尝试解析为通用组件命令（LAYOUT_CFG / COMP_LAYOUT / COMP_SET）
                if let Some(cmd) = parse::parse_component_packet(&rx_buf) {
                    let _ = DISPLAY_DATA.try_send(cmd);
                }

                // 尝试解析为主机确认弹窗请求
                if let Some(req) = parse::parse_dialog_packet(&rx_buf) {
                    let _ = DIALOG_DATA.try_send(req);
                    // 收到 ShowDialog 立即置拦截标志：弹窗在 display 循环 drain 前
                    // （首页最长 ~1s 帧间隔）输入不能漏到 keymap（音量/按键）
                    crate::menu::state::set_dialog_active(true);
                    crate::menu::set_rmk_menu_mode(true);
                }

                // 尝试处理控制命令（PING, GET_STATUS, GET_CAPABILITIES）
                if let Some(resp) = parse::handle_control_packet(&rx_buf, &current_config) {
                    let _ = DATA_CHANNEL_TX.try_send(resp);
                }
            }

            // 配置变化 → 更新本地跟踪 + 通知主机
            Either3::Second(config) => {
                current_config = config;
                let mut buf = [0u8; 64];
                if let Some(_n) = build_config_changed(&mut buf, &config) {
                    let _ = DATA_CHANNEL_TX.try_send(buf);
                    defmt::info!("Config changed: pad={}", config.active_pad);
                }
            }

            // 弹窗结果 → 回传主机
            Either3::Third(outcome) => {
                let mut buf = [0u8; 64];
                let code = DialogResultCode::from_u8(outcome.result);
                if let Some(code) = code {
                    if let Some(_n) = build_dialog_result(&mut buf, outcome.id, outcome.selection, code) {
                        let _ = DATA_CHANNEL_TX.try_send(buf);
                        defmt::info!("Dialog result: id={} result={}", outcome.id, outcome.result);
                    }
                }
            }
        }
    }
}
