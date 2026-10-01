// INPUT:  embassy_nrf(twim,gpio), driver::sh1107, settings, battery, menu, wououi, data_channel, mode, rmk
// OUTPUT: pub run_display() async task
// POS:    OLED 显示主循环，30FPS 菜单 / 1FPS 首页 / 数据通道渲染 / 屏幕自动休眠 / 主机确认弹窗

pub mod render;
pub mod icons;
pub mod format;

use embassy_nrf::gpio::{Level, Output, OutputDrive};
use embassy_nrf::peripherals::P0_06;
use embassy_nrf::twim::Twim;
use embassy_nrf::Peri;
use embassy_time::{Duration, Instant, Timer};

use crate::battery::{self, BATTERY_STATUS};
use crate::data_channel::{CompCache, DialogOutcome, DisplayDataCache, DIALOG_DATA, DIALOG_RESULT, DISPLAY_DATA};
use crate::driver;
use crate::driver::sh1107::Sh1107;
use crate::menu::{MenuInput, MENU_INPUT, MENU_STATE, MenuState, PageId};
use crate::mode::CURRENT_MODE;
use crate::settings::{SETTINGS, keys};
use crate::wououi::{WouoUI, WououiInput, SCREEN_WIDTH, SCREEN_HEIGHT};
use k9_datachannel_proto::DialogResultCode;
use render::{draw_keyboard_ui, draw_component_grid};
use rmk::types::ble::BleState;
use rmk::event::{ConnectionStatusChangeEvent, SubscribableEvent};

/// 亮度百分比转 SH1107 对比度寄存器值
const MIN_CONTRAST: u16 = 5;
fn brightness_to_contrast(brightness: u8) -> u8 {
    (MIN_CONTRAST + brightness as u16 * (255 - MIN_CONTRAST) / 100) as u8
}

/// 将菜单输入转换为 WouoUI 输入
fn menu_input_to_wououi(input: MenuInput) -> Option<WououiInput> {
    match input {
        MenuInput::ScrollUp => Some(WououiInput::Up),
        MenuInput::ScrollDown => Some(WououiInput::Down),
        MenuInput::Select => Some(WououiInput::Click),
        MenuInput::Back => Some(WououiInput::Return),
        MenuInput::EnterMenu => None, // 特殊处理
        MenuInput::ExitMenu => None,  // 特殊处理
    }
}

/// 显示任务主循环
pub async fn run_display(i2c: Twim<'static>, reset: Peri<'static, P0_06>) {
    // SAFETY: GPIO 寄存器访问，此时 I2C 外设尚未初始化，无竞争
    unsafe {
        driver::enable_i2c_pullups();
        driver::enable_oled_power();
    }

    // 等待电源稳定
    Timer::after(Duration::from_millis(500)).await;

    // 硬件复位 OLED
    defmt::info!("Resetting OLED...");
    let mut reset_pin = Output::new(reset, Level::High, OutputDrive::Standard);
    Timer::after(Duration::from_millis(100)).await;
    reset_pin.set_low();
    Timer::after(Duration::from_millis(100)).await;
    reset_pin.set_high();
    Timer::after(Duration::from_millis(100)).await;

    // 创建显示驱动
    let mut display = Sh1107::new(i2c);

    // 探测设备
    defmt::info!("Probing 0x3C...");
    match display.probe().await {
        Ok(_) => defmt::info!("Device found!"),
        Err(_) => {
            defmt::error!("Device NOT found!");
            loop {
                Timer::after(Duration::from_secs(1)).await;
            }
        }
    }

    // 初始化显示
    if let Err(_) = display.init().await {
        defmt::error!("Init failed!");
        loop {
            Timer::after(Duration::from_secs(1)).await;
        }
    }

    // 打开显示
    Timer::after(Duration::from_millis(200)).await;
    display.send_command(0xAF).await.ok();
    defmt::info!("Display ON (firmware {})", env!("K9_GIT_HASH"));

    // 初始化 WouoUI（传入帧间隔，自动适配 blur 时序）
    const MENU_FRAME_MS: u16 = 8; // ~125 FPS
    let mut wououi = WouoUI::new();
    wououi.init(MENU_FRAME_MS);

    // 从 flash 恢复亮度设置
    let saved_brightness = SETTINGS.read(keys::BRIGHTNESS, 80);
    wououi.set_brightness(saved_brightness);
    {
        let contrast = brightness_to_contrast(saved_brightness);
        display.set_contrast(contrast).await.ok();
        defmt::info!("Restored brightness: {}% (contrast={})", saved_brightness, contrast);
    }

    // 从 flash 恢复屏幕超时设置
    let saved_timeout = SETTINGS.read(keys::SCREEN_TIMEOUT, 20);
    wououi.set_screen_timeout(saved_timeout);
    let mut screen_timeout_secs: u8 = saved_timeout;
    let mut confirmed_screen_timeout: u8 = saved_timeout;

    // 从 flash 恢复 Quick Menu 设置
    let saved_quick_menu = SETTINGS.read(keys::QUICK_MENU, 1);
    wououi.set_quick_menu_enabled(saved_quick_menu != 0);
    let mut confirmed_quick_menu: bool = saved_quick_menu != 0;
    defmt::info!("Restored quick_menu: {}", saved_quick_menu != 0);

    // 当前 BLE profile：开机先默认 0，随后通过 ConnectionStatusChangeEvent 被动同步。
    // RMK 0.10 把旧的 pub static ACTIVE_PROFILE 收成了 pub(crate) fn current_profile()
    // （refactor/connection_status），设计意图是让外部通过事件订阅感知 profile 变化。
    // RMK 在 storage init 从 flash 加载 profile 后会触发 ConnectionStatusChangeEvent，
    // 下面 ble_sub 那条 select 分支会消费它，把 UI 更新到正确值。
    wououi.set_selected_user(0);
    let mut current_user: u8 = 0;

    // 屏幕睡眠状态
    let mut screen_on = true;
    let mut last_screen_activity = Instant::now();

    // 菜单状态跟踪
    let mut menu_active = false;
    let mut menu_idle_ticks: u16 = 0;
    const MENU_TIMEOUT_TICKS: u16 = (1000 / MENU_FRAME_MS) * 30; // 30秒

    // 弹窗独立超时（5 分钟兜底）：弹窗锁屏不受菜单空闲超时影响
    let mut dialog_ticks: u16 = 0;
    const DIALOG_TIMEOUT_TICKS: u16 = (1000 / MENU_FRAME_MS) * 300; // 5分钟

    // 主机确认弹窗状态（BLE ShowDialog）
    // 弹窗期间 menu_active 保持 true，按键经现有菜单路由零改动复用
    let mut dialog_active = false;
    // 新弹窗（选项选择器）：标题 + 选项 + 当前选中项（滚轮切换）
    let mut dialog_kind = k9_datachannel_proto::DialogKind::ConfirmCancel;
    let mut dialog_title: heapless::String<56> = heapless::String::new();
    let mut dialog_options: [Option<heapless::String<56>>; 4] = [const { None }; 4];
    let mut dialog_selection: u8 = 0;
    let mut dialog_id: u8 = 0;

    // 组件网格缓存（新架构：host 声明 LAYOUT_CFG/COMP_LAYOUT，设备本地渲染）；
    // 旧 slot 缓存保留做 fallback（host 全切组件后移除）
    let mut comp_cache = CompCache::new();
    let mut dc_cache = DisplayDataCache::new();

    // 获取状态发送器和接收器（电池由 battery 任务采样，这里只订阅）
    let menu_state_tx = MENU_STATE.sender();
    let mut battery_rx = BATTERY_STATUS.receiver().unwrap();
    let mode_tx = CURRENT_MODE.sender();

    // 初始状态
    let mut current_mode = crate::mode::KeyboardMode::default();
    mode_tx.send(current_mode);
    let mut current_pad_index: u8 = 0;
    let mut current_brightness: u8 = saved_brightness;
    let mut confirmed_brightness: u8 = saved_brightness;
    let mut last_contrast_write = Instant::now();
    const CONTRAST_MIN_INTERVAL: Duration = Duration::from_millis(100);
    // BLE 连接状态：通过 RMK 事件系统订阅（替代有 bug 的 get_connection_state 轮询）
    let mut ble_sub = ConnectionStatusChangeEvent::subscriber();
    let mut ble_connected = false;

    // 电池状态由 battery 任务异步采样并经 BATTERY_STATUS 广播；这里只读最新值。
    // 启动初期(任务首次采样完成前)显示默认值，随后几十 ms 内被刷新。
    let mut battery_status = battery::BatteryStatus::default();


    // 发送初始菜单状态
    let initial_state = MenuState {
        active: false,
        current_page: PageId::Home,
        selected_index: 0,
        scroll_offset: 0,
        target_scroll_offset: 0,
    };
    menu_state_tx.send(initial_state);

    // 用于计算帧间隔
    let mut last_frame = Instant::now();

    // 启动菜单控制器（并行运行）
    let mut menu_ctrl = crate::menu::MenuController::new();
    let menu_ctrl_future = menu_ctrl.run();

    // 显示主循环
    let display_future = async {
    // 变更检测状态（跨帧保持，避免 static mut）
    let mut prev_active_pad: u8 = 0xFF;
    let mut prev_active: bool = false;

    loop {
        let now = Instant::now();
        let elapsed_ms = (now - last_frame).as_millis() as u16;
        last_frame = now;

        // 非阻塞方式处理输入事件
        while let Ok(input) = MENU_INPUT.try_receive() {
            defmt::info!("Menu input: {:?}", defmt::Debug2Format(&input));

            // 屏幕关闭时，任何输入唤醒屏幕但不转发给菜单系统
            if !screen_on {
                let quick_menu_trigger = input == MenuInput::EnterMenu && wououi.get_quick_menu_enabled();
                defmt::info!("Screen wake: input while screen off");
                display.send_command(0xAF).await.ok(); // Display ON
                display.set_contrast(brightness_to_contrast(current_brightness)).await.ok();
                screen_on = true;
                last_screen_activity = now;
                if quick_menu_trigger {
                    menu_active = true;
                    wououi.enter_menu();
                    menu_idle_ticks = 0;
                    defmt::info!("Quick menu: wake + enter menu");
                }
                continue; // consume input, don't forward
            }

            // 重置空闲计时（屏幕开启时）
            menu_idle_ticks = 0;
            last_screen_activity = now;

            // 弹窗模式：滚轮切选项，Select/Back 确认/取消（劫持，不转发 wououi）
            if dialog_active {
                let count = dialog_options.iter().filter(|o| o.is_some()).count() as u8;
                match input {
                    MenuInput::ScrollUp => {
                        if count > 0 {
                            dialog_selection = (dialog_selection + count - 1) % count;
                        }
                    }
                    MenuInput::ScrollDown => {
                        if count > 0 {
                            dialog_selection = (dialog_selection + 1) % count;
                        }
                    }
                    MenuInput::Select => {
                        let _ = DIALOG_RESULT.try_send(DialogOutcome {
                            id: dialog_id,
                            selection: dialog_selection,
                            result: DialogResultCode::Confirm as u8,
                        });
                        defmt::info!("Dialog confirmed: id={} selection={}", dialog_id, dialog_selection);
                        dialog_active = false;
                        crate::menu::state::set_dialog_active(false);
                        crate::menu::set_rmk_menu_mode(menu_active);
                        last_screen_activity = now; // 弹窗关闭后不立刻休眠
                    }
                    MenuInput::Back => {
                        let _ = DIALOG_RESULT.try_send(DialogOutcome {
                            id: dialog_id,
                            selection: 0,
                            result: DialogResultCode::Cancel as u8,
                        });
                        defmt::info!("Dialog cancelled: id={}", dialog_id);
                        dialog_active = false;
                        crate::menu::state::set_dialog_active(false);
                        crate::menu::set_rmk_menu_mode(menu_active);
                        last_screen_activity = now; // 弹窗关闭后不立刻休眠
                    }
                    _ => {}
                }
                continue;
            }

            match input {
                MenuInput::EnterMenu => {
                    if !menu_active {
                        menu_active = true;
                        wououi.enter_menu();
                        defmt::info!("WouoUI: Menu activated");
                    }
                }
                MenuInput::ExitMenu => {
                    if menu_active {
                        menu_active = false;
                        wououi.exit_menu();
                        defmt::info!("WouoUI: Menu deactivated");
                    }
                }
                MenuInput::Back => {
                    // 在主页按返回键：退出菜单
                    // 在子页面按返回键：返回上一级
                    if menu_active {
                        if wououi.is_on_home_page() {
                            menu_active = false;
                            wououi.exit_menu();
                            defmt::info!("WouoUI: Back on home page -> exit menu");
                        } else {
                            wououi.send_input(WououiInput::Return);
                        }
                    }
                }
                _ => {
                    // 转换为 WouoUI 输入（ScrollUp, ScrollDown, Select）
                    if menu_active {
                        if let Some(wououi_input) = menu_input_to_wououi(input) {
                            wououi.send_input(wououi_input);
                        }
                    }
                }
            }
        }

        // 数据通道接收唤醒屏幕
        if !screen_on {
            // try_peek 只探测不消费：旧版 try_receive 把唤醒检测吃掉了第一条显示命令
            //（注释自己写着 consumed command is lost）——睡眠时收到的第一条消息永远显示
            // 不出来。改为 peek：命令保留在通道里，本帧后续 drain 正常吸收并渲染。
            if let Ok(_) = DISPLAY_DATA.try_peek() {
                defmt::info!("Screen wake: data channel received while screen off");
                display.send_command(0xAF).await.ok(); // Display ON
                display.set_contrast(brightness_to_contrast(current_brightness)).await.ok();
                screen_on = true;
                last_screen_activity = now;
            }
        }

        // 主机弹窗请求（ShowDialog）：强制锁屏 + 重置选项 + 进入弹窗模式。
        // 选项由随后的 DialogOption 命令逐条填充（见 DISPLAY_DATA drain）。
        while let Ok(req) = DIALOG_DATA.try_receive() {
            if dialog_active {
                // v1 取舍：新弹窗顶掉未决旧弹窗，旧的按 Timeout 回传
                let _ = DIALOG_RESULT.try_send(DialogOutcome {
                    id: dialog_id,
                    selection: 0,
                    result: DialogResultCode::Timeout as u8,
                });
                defmt::info!("Host dialog superseded: id={} -> Timeout", dialog_id);
            }
            if !screen_on {
                defmt::info!("Screen wake: dialog request while screen off");
                display.send_command(0xAF).await.ok(); // Display ON
                display.set_contrast(brightness_to_contrast(current_brightness)).await.ok();
                screen_on = true;
                last_screen_activity = now;
            }
            dialog_active = true;
            crate::menu::state::set_dialog_active(true);
            // 弹窗期间吞掉滚轮/按键（不调音量、不进菜单），交给弹窗选择器
            crate::menu::set_rmk_menu_mode(true);
            dialog_id = req.id;
            dialog_kind = req.kind;
            dialog_title = req.title;
            dialog_options = [const { None }; 4];
            dialog_selection = 0;
            dialog_ticks = 0;
            menu_idle_ticks = 0;
            defmt::info!("Host dialog show: id={} kind={:?}", dialog_id, defmt::Debug2Format(&dialog_kind));
        }

        // 非阻塞消费显示数据命令（任何模式都处理：组件值 / 弹窗选项 / wake）
        while let Ok(cmd) = DISPLAY_DATA.try_receive() {
            comp_cache.apply(&cmd);
            dc_cache.apply(&cmd);
            // 弹窗选项：ShowDialog 之后逐条到达，攒进弹窗缓存
            if let crate::data_channel::DisplayCommand::DialogOption { id, index, label } = &cmd {
                if dialog_active && *id == dialog_id && (*index as usize) < dialog_options.len() {
                    dialog_options[*index as usize] = Some(label.clone());
                    defmt::info!("Dialog option id={} index={}: {}", id, index, label.as_str());
                }
            }
            // wake 标志：屏幕关着时唤醒显示（新通知等）
            if matches!(cmd, crate::data_channel::DisplayCommand::SetCompValue { wake: true, .. })
                && !screen_on
            {
                defmt::info!("Screen wake: wake=true component push");
                display.send_command(0xAF).await.ok(); // Display ON
                display.set_contrast(brightness_to_contrast(current_brightness)).await.ok();
                screen_on = true;
            }
            last_screen_activity = now; // 数据通道活动重置超时
        }

        // 从 battery 任务读取最新电池状态（非阻塞，每帧检查一次）
        if let Some(s) = battery_rx.try_changed() {
            battery_status = s;
        }

        // ====== BLE 状态 + Profile 同步：非阻塞消费事件 ======
        // ConnectionStatusChangeEvent payload 是 ConnectionStatus { ble: { state, profile }, ... }
        // BLE 连接状态 + RMK 内部 profile 变化都通过这一个事件流过来
        while let Some(event) = ble_sub.try_next_message_pure() {
            let ble = event.0.ble;

            // BLE 连接状态
            let new_connected = matches!(ble.state, BleState::Connected);
            if new_connected != ble_connected {
                defmt::info!(
                    ">>> BLE event: {:?}, connected: {} -> {}",
                    defmt::Debug2Format(&ble.state),
                    ble_connected,
                    new_connected
                );
                ble_connected = new_connected;
            }

            // Profile 同步：RMK 在 storage init 加载完成 / 用户通过 switch_ble_profile
            // 切换时都会触发这个事件，从这里被动跟上 UI 状态。
            if ble.profile != current_user {
                defmt::info!("BLE profile sync: {} -> {}", current_user, ble.profile);
                current_user = ble.profile;
                wououi.set_selected_user(ble.profile);
            }
        }

        // 空闲计时（主循环，任何模式都跑——弹窗/菜单超时与渲染分支解耦）
        if dialog_active {
            dialog_ticks += 1;
            if dialog_ticks > DIALOG_TIMEOUT_TICKS {
                let _ = DIALOG_RESULT.try_send(DialogOutcome {
                    id: dialog_id,
                    selection: dialog_selection,
                    result: DialogResultCode::Timeout as u8,
                });
                defmt::info!("Host dialog timeout (5min): id={}", dialog_id);
                dialog_active = false;
                crate::menu::state::set_dialog_active(false);
                crate::menu::set_rmk_menu_mode(menu_active);
                dialog_ticks = 0;
                last_screen_activity = now;
            }
        } else {
            menu_idle_ticks += 1;
            if menu_idle_ticks > MENU_TIMEOUT_TICKS {
                menu_active = false;
                wououi.exit_menu();
                defmt::info!("WouoUI: Menu timeout, returning to home");
            }
        }

        // 渲染 + 刷新（仅在屏幕开启时）
        if screen_on {
        if dialog_active {
            // 弹窗选择器：标题 + 选项列表（滚轮切选项，Select 确认）
            crate::display::render::draw_dialog(
                &mut display,
                dialog_title.as_str(),
                &dialog_options,
                dialog_selection,
            );
        } else if menu_active {
            // 菜单模式：使用 WouoUI 渲染
            // 限制帧间隔在合理范围，防止从低帧率(首页1FPS)切换时
            // 过大的 elapsed_ms 导致动画计算异常
            let clamped_elapsed = elapsed_ms.clamp(1, 50);
            let screen_updated = wououi.tick(clamped_elapsed);

            if screen_updated {
                if let Some(buffer) = wououi.get_buffer() {
                    display.copy_from_wououi(buffer, SCREEN_WIDTH, SCREEN_HEIGHT);
                }
            }

            // C 回调请求退出菜单（如 Pad 选择后）
            if wououi.take_exit_request() {
                menu_active = false;
                wououi.exit_menu();
                defmt::info!("WouoUI: Exit requested by callback");
            }

            // C 回调请求进入 DFU 模式（Settings -> DFU Mode）
            if wououi.take_dfu_request() {
                defmt::info!("DFU mode requested, jumping to bootloader...");
                // 0xA8 → Adafruit bootloader 进入 BLE OTA DFU
                embassy_nrf::pac::POWER
                    .gpregret()
                    .write_value(embassy_nrf::pac::power::regs::Gpregret(0xA8));
                cortex_m::peripheral::SCB::sys_reset();
            }

            // （旧 wououi 弹窗路径已由新选项选择器替代——ShowDialog 走 DIALOG_DATA 分支）

            // C 回调请求进入 USB Bootloader（Settings -> To Bootloader）
            if wououi.take_usb_bl_request() {
                defmt::info!("USB bootloader requested, resetting...");
                // 写 0x57 到 GPREGRET 寄存器，Adafruit bootloader 识别后进入 USB UF2 DFU
                embassy_nrf::pac::POWER
                    .gpregret()
                    .write_value(embassy_nrf::pac::power::regs::Gpregret(0x57));
                cortex_m::peripheral::SCB::sys_reset();
            }

            // 检测 Pad 选择变化，切换 RMK Layer
            let selected_pad = wououi.get_selected_pad();
            if selected_pad != current_pad_index {
                current_pad_index = selected_pad;
                let mode = crate::mode::KeyboardMode::from_layer(selected_pad);
                current_mode = mode;
                rmk::controller::set_default_layer(selected_pad);
                // 广播模式变更
                mode_tx.send(mode);
                defmt::info!("Pad switched to {} (layer {})", mode.name(), selected_pad);
            }

            // 实时亮度预览：读取 ValWin 滑块实时值，限速 100ms
            let brightness = wououi.get_live_brightness();
            if brightness != current_brightness {
                if now.duration_since(last_contrast_write) >= CONTRAST_MIN_INTERVAL {
                    current_brightness = brightness;
                    last_contrast_write = now;
                    let contrast = brightness_to_contrast(brightness);
                    if let Err(_) = display.set_contrast(contrast).await {
                        defmt::error!("Failed to set contrast");
                    }
                    defmt::info!("Brightness: {}% (contrast={})", brightness, contrast);
                }
            }

            // 持久化亮度：检测确认值变化（非实时滑块预览）写入 flash
            let confirmed = wououi.get_brightness();
            if confirmed != confirmed_brightness {
                confirmed_brightness = confirmed;
                SETTINGS.write(keys::BRIGHTNESS, confirmed);
            }

            // 持久化屏幕超时：检测 ListWin 确认值变化
            let timeout = wououi.get_screen_timeout();
            if timeout != confirmed_screen_timeout {
                confirmed_screen_timeout = timeout;
                screen_timeout_secs = timeout;
                SETTINGS.write(keys::SCREEN_TIMEOUT, timeout);
                defmt::info!("Screen timeout changed: {}s", timeout);
            }

            // 持久化 Quick Menu 设置
            let quick_menu = wououi.get_quick_menu_enabled();
            if quick_menu != confirmed_quick_menu {
                confirmed_quick_menu = quick_menu;
                SETTINGS.write(keys::QUICK_MENU, if quick_menu { 1 } else { 0 });
                defmt::info!("Quick menu changed: {}", quick_menu);
            }

            // 检测 User 切换（BLE 多设备）
            let selected_user = wououi.get_selected_user();
            if selected_user != current_user {
                current_user = selected_user;
                rmk::controller::switch_ble_profile(selected_user);
                defmt::info!("User switched to User {} (profile {})", selected_user, selected_user);
            }

            // 检测 Clear Bond 请求
            if wououi.take_clear_bond_request() {
                rmk::controller::clear_ble_bond();
                defmt::info!("Clear bond for User {} (profile {})", current_user, current_user);
            }

            // 检测 重置请求（确认弹窗已通过，C 侧置位）。每个都在落盘后软复位生效。
            if wououi.take_reset_keys_request() {
                defmt::info!("Reset keyboard config (keymap, keep bonds/app settings)");
                rmk::request_keyboard_config_reset().await;
                cortex_m::peripheral::SCB::sys_reset();
            }
            if wououi.take_reset_app_request() {
                defmt::info!("Reset app settings (FlashStore)");
                SETTINGS.erase();
                cortex_m::peripheral::SCB::sys_reset();
            }
            if wououi.take_erase_all_request() {
                defmt::info!("Erase all (RMK storage + app settings)");
                SETTINGS.erase();
                // 只是把擦除请求排进 RMK 存储任务；它擦完整个存储区（含配对）后自己重启。
                // 这里不能立刻 sys_reset，否则可能在擦除前就复位。10s 兜底远大于擦除耗时。
                rmk::reset_all_storage().await;
                Timer::after_secs(10).await;
                cortex_m::peripheral::SCB::sys_reset();
            }

            // 检测 active_pad 变化，通知主机
            // enabled_functions 语义已废弃（显示由 host 推送驱动），恒定上报 0xFFFF 保持 wire 兼容
            if current_pad_index != prev_active_pad {
                prev_active_pad = current_pad_index;
                let new_config = k9_datachannel_proto::PadConfig {
                    active_pad: current_pad_index,
                    enabled_functions: 0xFFFF,
                };
                crate::data_channel::DATA_CHANNEL_CONFIG.sender().send(new_config);
                defmt::info!("DC config: pad={}", new_config.active_pad);
            }

        } else {
            // 屏幕自动休眠：仅在首页（非菜单）时检测超时；弹窗激活时锁屏不睡
            if !dialog_active
                && now.duration_since(last_screen_activity) >= Duration::from_secs(screen_timeout_secs as u64) {
                defmt::info!("Screen sleep: timeout {}s reached", screen_timeout_secs);
                display.send_command(0xAE).await.ok(); // Display OFF
                screen_on = false;
            }

            // 首页布局：新架构 = 组件网格（host 声明布局 + 实时推值）；
            // 无组件时回退键盘状态页
            let has_comps = comp_cache.comps.iter().any(|c| c.is_some());
            if has_comps {
                draw_component_grid(
                    &mut display,
                    current_mode.name(),
                    battery_status.percentage,
                    ble_connected,
                    &comp_cache,
                );
            } else {
                draw_keyboard_ui(
                    &mut display,
                    current_mode.name(),
                    battery_status.percentage,
                    ble_connected,
                );
            }
        }

        // 刷新到屏幕
        if let Err(_) = display.flush().await {
            defmt::error!("Display flush failed");
        }
        } // end if screen_on

        // 广播菜单状态（仅在 active 变化时发送，避免每帧都广播）
        {
            let new_active = menu_active;
            if new_active != prev_active {
                prev_active = new_active;

                // 同步 RMK 菜单模式标志，控制按键/编码器拦截
                crate::menu::set_rmk_menu_mode(new_active);
                let state = MenuState {
                    active: new_active,
                    current_page: if new_active { PageId::MainMenu } else { PageId::Home },
                    selected_index: 0,
                    scroll_offset: 0,
                    target_scroll_offset: 0,
                };
                menu_state_tx.send(state);
            }
        }

        // 动态帧率：菜单模式 ~125 FPS，首页 1 FPS，屏幕关闭 200ms 轮询
        let frame_delay = if !screen_on {
            Duration::from_millis(200) // 200ms 轮询，响应唤醒事件
        } else if menu_active {
            Duration::from_millis(MENU_FRAME_MS as u64) // ~125 FPS
        } else {
            Duration::from_millis(1000) // 1 FPS
        };

        Timer::after(frame_delay).await;
    }
    }; // end display_future

    // 并行运行显示和菜单控制器
    rmk::embassy_futures::join::join(display_future, menu_ctrl_future).await;
}
