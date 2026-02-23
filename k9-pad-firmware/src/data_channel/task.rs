// INPUT:  rmk::data_channel, super::{parse, DISPLAY_DATA, DATA_CHANNEL_CONFIG}
// OUTPUT: run_data_channel() async task
// POS:    数据通道主任务，桥接 RMK BLE/USB 收发与显示命令分发

use super::{parse, DISPLAY_DATA, DATA_CHANNEL_CONFIG};

/// 数据通道处理主任务
///
/// 从 RMK 的 DATA_CHANNEL_RX 接收主机数据，解析协议，
/// 分发 DisplayCommand 到 DISPLAY_DATA channel。
/// 同时监听菜单配置变化，发送 CONFIG_CHANGED 到 DATA_CHANNEL_TX。
#[cfg(not(test))]
pub async fn run_data_channel() -> ! {
    use k9_datachannel_proto::build_config_changed;
    use rmk::data_channel::{DATA_CHANNEL_RX, DATA_CHANNEL_TX};

    defmt::info!("Data channel task started");

    // 配置变化监听
    let mut config_rx = DATA_CHANNEL_CONFIG
        .receiver()
        .expect("DATA_CHANNEL_CONFIG: no receiver slot available (max 2)");

    loop {
        // 同时等待：主机数据 或 配置变化
        match rmk::embassy_futures::select::select(
            DATA_CHANNEL_RX.receive(),
            config_rx.changed(),
        )
        .await
        {
            // 收到主机数据
            rmk::embassy_futures::select::Either::First(rx_buf) => {
                // 尝试解析为显示命令
                if let Some(cmd) = parse::parse_display_packet(&rx_buf) {
                    let _ = DISPLAY_DATA.try_send(cmd);
                }

                // 尝试处理控制命令（PING, GET_STATUS）
                if let Some(resp) = parse::handle_control_packet(&rx_buf) {
                    let _ = DATA_CHANNEL_TX.try_send(resp);
                }
            }

            // 配置变化 → 通知主机
            rmk::embassy_futures::select::Either::Second(config) => {
                let mut buf = [0u8; 64];
                if let Some(_n) = build_config_changed(&mut buf, &config) {
                    let _ = DATA_CHANNEL_TX.try_send(buf);
                    defmt::info!(
                        "Config changed: pad={} functions=0x{:04x}",
                        config.active_pad,
                        config.enabled_functions
                    );
                }
            }
        }
    }
}
