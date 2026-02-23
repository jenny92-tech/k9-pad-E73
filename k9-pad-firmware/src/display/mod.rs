// INPUT:  embassy_nrf(twim,gpio), driver::sh1107, settings, battery, menu, wououi, data_channel, mode, rmk
// OUTPUT: pub run_display() async task
// POS:    OLED 显示主循环，30FPS 菜单 / 1FPS 首页 / 数据通道渲染 / 屏幕自动休眠

pub mod render;
pub mod icons;
pub mod format;

use embassy_nrf::gpio::{Level, Output, OutputDrive};
use embassy_nrf::peripherals::P0_06;
use embassy_nrf::twim::Twim;
use embassy_nrf::Peri;
use embassy_time::{Duration, Instant, Timer};

use crate::battery::{self, BATTERY_STATUS};
use crate::data_channel::{DisplayDataCache, DISPLAY_DATA};
use crate::driver;
use crate::driver::sh1107::Sh1107;
use crate::menu::{MenuInput, MENU_INPUT, MENU_STATE, MenuState, PageId};
use crate::mode::CURRENT_MODE;
use crate::settings::{SETTINGS, keys};
use crate::wououi::{WouoUI, WououiInput, SCREEN_WIDTH, SCREEN_HEIGHT};
use render::{draw_keyboard_ui, draw_data_channel_ui};
use rmk::ble::BleState;
use rmk::event::{BleStateChangeEvent, SubscribableEvent};

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
    defmt::info!("Display ON");

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

    // 从 flash 恢复每个 Pad 的数据通道设置
    let mut confirmed_dc_functions: [u16; 5] = [0; 5];
    for pad in 0..5u8 {
        let mask = SETTINGS.read(keys::DC_FUNCTIONS_PAD0 + pad, 0);
        if mask != 0 {
            wououi.set_enabled_functions(pad, mask as u16);
            confirmed_dc_functions[pad as usize] = mask as u16;
            defmt::info!("Restored dc_functions[{}] = 0x{:02x}", pad, mask);
        }
    }

    // 从 flash 恢复屏幕超时设置
    let saved_timeout = SETTINGS.read(keys::SCREEN_TIMEOUT, 20);
    wououi.set_screen_timeout(saved_timeout);
    let mut screen_timeout_secs: u8 = saved_timeout;
    let mut confirmed_screen_timeout: u8 = saved_timeout;

    // 从 flash 恢复 Quick Menu 设置
    let saved_quick_menu = SETTINGS.read(keys::QUICK_MENU, 0);
    wououi.set_quick_menu_enabled(saved_quick_menu != 0);
    let mut confirmed_quick_menu: bool = saved_quick_menu != 0;
    defmt::info!("Restored quick_menu: {}", saved_quick_menu != 0);

    // 屏幕睡眠状态
    let mut screen_on = true;
    let mut last_screen_activity = Instant::now();

    // 菜单状态跟踪
    let mut menu_active = false;
    let mut menu_idle_ticks: u16 = 0;
    const MENU_TIMEOUT_TICKS: u16 = (1000 / MENU_FRAME_MS) * 30; // 30秒

    // 数据通道显示缓存 + slot 轮播
    let mut dc_cache = DisplayDataCache::new();
    let mut dc_current_slot: u8 = 0;
    let mut dc_rotation_timer = Instant::now();
    const DC_ROTATION_INTERVAL: Duration = Duration::from_secs(4);

    // 初始化充电检测引脚 (P0.07, 上拉输入)
    // SAFETY: P0.07 未被其他任务使用，见 init_charge_detect_pin 文档
    unsafe { battery::init_charge_detect_pin() };
    defmt::info!("Battery: charge detect pin (P0.07) initialized");

    // 获取状态发送器和接收器
    let menu_state_tx = MENU_STATE.sender();
    let battery_status_tx = BATTERY_STATUS.sender();
    let mode_tx = CURRENT_MODE.sender();

    // 初始状态
    let mut current_mode = crate::mode::KeyboardMode::default();
    mode_tx.send(current_mode);
    let mut current_pad_index: u8 = 0;
    let mut current_brightness: u8 = saved_brightness;
    let mut confirmed_brightness: u8 = saved_brightness;
    let mut last_contrast_write = Instant::now();
    const CONTRAST_MIN_INTERVAL: Duration = Duration::from_millis(100);
    let mut current_ble_enabled: bool = true;
    let mut current_user: u8 = 0;
    // BLE 连接状态：通过 RMK 事件系统订阅（替代有 bug 的 get_connection_state 轮询）
    let mut ble_sub = BleStateChangeEvent::subscriber();
    let mut ble_connected = false;

    // 首次读取电池状态（避免启动后 5 秒内显示 0%）
    let mut battery_status = {
        let voltage = battery::read_battery_voltage_mv();
        let percentage = battery::calc_percentage(voltage);
        let is_charging = unsafe { battery::read_charge_pin() };
        let status = battery::BatteryStatus {
            voltage_mv: voltage,
            percentage,
            is_charging,
        };
        battery_status_tx.send(status);
        defmt::info!("Battery init: {}mV {}% charging={}", voltage, percentage, is_charging);
        status
    };

    // 电池读取计时
    let mut last_battery_read = Instant::now();
    const BATTERY_READ_INTERVAL: Duration = Duration::from_secs(5);
    // EMA 平滑用的累积值 (×10 精度，避免浮点)
    let mut smooth_pct_x10: u16 = battery_status.percentage as u16 * 10;

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
            if let Ok(_) = DISPLAY_DATA.try_receive() {
                defmt::info!("Screen wake: data channel received while screen off");
                display.send_command(0xAF).await.ok(); // Display ON
                display.set_contrast(brightness_to_contrast(current_brightness)).await.ok();
                screen_on = true;
                last_screen_activity = now;
                // Note: the consumed data command is lost, but next loop iteration picks up more
            }
        }

        // 更新电池状态（每 5 秒直接读取 SAADC + 充电引脚）
        if now.duration_since(last_battery_read) >= BATTERY_READ_INTERVAL {
            last_battery_read = now;

            let voltage = battery::read_battery_voltage_mv();
            let raw_pct = battery::calc_percentage(voltage);
            let is_charging = unsafe { battery::read_charge_pin() };

            // EMA 平滑：smooth = raw * 3 + prev * 7（α ≈ 0.3）
            // 使用 ×10 精度避免整数截断累积误差
            // 充电时不平滑（允许快速上升显示充电进度）
            let smoothed_pct = if is_charging {
                smooth_pct_x10 = raw_pct as u16 * 10;
                raw_pct
            } else {
                smooth_pct_x10 =
                    (raw_pct as u16 * 10 * 3 + smooth_pct_x10 * 7 + 5) / 10;
                ((smooth_pct_x10 + 5) / 10) as u8
            };

            battery_status = battery::BatteryStatus {
                voltage_mv: voltage,
                percentage: smoothed_pct,
                is_charging,
            };

            // 广播给其他消费者（如未来的 BLE battery service）
            battery_status_tx.send(battery_status);

            defmt::info!(
                "Battery: {}mV raw={}% smooth={}% charging={}",
                voltage,
                raw_pct,
                smoothed_pct,
                is_charging
            );

        }

        // ====== BLE 状态：非阻塞消费事件 ======
        while let Some(event) = ble_sub.try_next_message_pure() {
            let new_connected = matches!(event.state, BleState::Connected);
            if new_connected != ble_connected {
                defmt::info!(
                    ">>> BLE event: {:?}, connected: {} -> {}",
                    defmt::Debug2Format(&event.state),
                    ble_connected,
                    new_connected
                );
                ble_connected = new_connected;
            }
        }

        // 渲染 + 刷新（仅在屏幕开启时）
        if screen_on {
        if menu_active {
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
                rmk::set_default_layer(selected_pad);
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

            // 检测 BLE 开关变化
            let ble_enabled = wououi.get_ble_enabled();
            if ble_enabled != current_ble_enabled {
                current_ble_enabled = ble_enabled;
                defmt::info!("BLE enabled: {}", ble_enabled);
                // TODO: RMK 未暴露 BLE 启停的公共 API，待后续支持
            }

            // 检测 User 切换（BLE 多设备）
            let selected_user = wououi.get_selected_user();
            if selected_user != current_user {
                current_user = selected_user;
                rmk::switch_ble_profile(selected_user);
                defmt::info!("User switched to User {} (profile {})", selected_user, selected_user);
            }

            // 检测数据通道配置变化，通知主机
            {
                let dc_enabled = wououi.is_data_channel_enabled(current_pad_index);
                let functions = if dc_enabled {
                    wououi.get_enabled_functions(current_pad_index)
                } else {
                    0
                };
                let new_config = k9_datachannel_proto::PadConfig {
                    active_pad: current_pad_index,
                    enabled_functions: functions,
                };
                static mut PREV_DC_CONFIG: k9_datachannel_proto::PadConfig =
                    k9_datachannel_proto::PadConfig {
                        active_pad: 0xFF,
                        enabled_functions: 0xFFFF,
                    };
                // SAFETY: 单线程 display task 内部使用
                let prev = unsafe { PREV_DC_CONFIG };
                if new_config != prev {
                    unsafe { PREV_DC_CONFIG = new_config };
                    crate::data_channel::DATA_CHANNEL_CONFIG.sender().send(new_config);
                    defmt::info!(
                        "DC config: pad={} functions=0x{:04x}",
                        new_config.active_pad,
                        new_config.enabled_functions
                    );
                }
            }

            // 持久化数据通道设置：回到主菜单时保存（子页面设置已确认）
            {
                let on_home = wououi.is_on_home_page();
                static mut PREV_ON_HOME: bool = true;
                // SAFETY: 单线程 display task 内部使用
                let was_on_home = unsafe { PREV_ON_HOME };
                if on_home && !was_on_home {
                    // 刚从子页面返回主菜单，保存变更的设置
                    for pad in 0..5u8 {
                        let funcs = wououi.get_enabled_functions(pad);
                        if funcs != confirmed_dc_functions[pad as usize] {
                            confirmed_dc_functions[pad as usize] = funcs;
                            SETTINGS.write(keys::DC_FUNCTIONS_PAD0 + pad, funcs as u8);
                        }
                    }
                }
                unsafe { PREV_ON_HOME = on_home };
            }

            // 更新空闲计时器
            menu_idle_ticks += 1;
            if menu_idle_ticks > MENU_TIMEOUT_TICKS {
                menu_active = false;
                wououi.exit_menu();
                defmt::info!("WouoUI: Menu timeout, returning to home");
            }
        } else {
            // 非阻塞消费显示数据命令
            while let Ok(cmd) = DISPLAY_DATA.try_receive() {
                dc_cache.apply(&cmd);
                last_screen_activity = now; // 数据通道活动重置超时
            }

            // 屏幕自动休眠：仅在首页（非菜单）时检测超时
            if now.duration_since(last_screen_activity) >= Duration::from_secs(screen_timeout_secs as u64) {
                defmt::info!("Screen sleep: timeout {}s reached", screen_timeout_secs);
                display.send_command(0xAE).await.ok(); // Display OFF
                screen_on = false;
            }

            // 检查当前 Pad 是否启用了数据通道
            let dc_enabled = wououi.is_data_channel_enabled(current_pad_index);
            let active_slots = dc_cache.active_count();

            if dc_enabled && active_slots > 0 {
                // 模式 2：数据通道布局（浮动头部 + 内容区）
                // Slot 轮播：多个 slot 时每 4 秒切换
                if active_slots > 1
                    && now.duration_since(dc_rotation_timer) >= DC_ROTATION_INTERVAL
                {
                    dc_rotation_timer = now;
                    // 找下一个有数据的 slot
                    for _ in 0..8 {
                        dc_current_slot = (dc_current_slot + 1) % 8;
                        if dc_cache.slots[dc_current_slot as usize].is_some() {
                            break;
                        }
                    }
                }

                // 确保当前 slot 有数据（可能被 clear 了）
                if dc_cache.slots[dc_current_slot as usize].is_none() {
                    // 找第一个有数据的 slot
                    for i in 0..8u8 {
                        if dc_cache.slots[i as usize].is_some() {
                            dc_current_slot = i;
                            break;
                        }
                    }
                }

                let slot_data = dc_cache.slots[dc_current_slot as usize].as_ref();
                draw_data_channel_ui(
                    &mut display,
                    current_mode.name(),
                    battery_status.percentage,
                    ble_connected,
                    slot_data,
                );
            } else {
                // 模式 1：居中显示（无数据通道功能启用）
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
            static mut PREV_ACTIVE: bool = false;
            // SAFETY: 单线程 display task 内部使用，无竞争
            let prev = unsafe { PREV_ACTIVE };
            if new_active != prev {
                unsafe { PREV_ACTIVE = new_active };

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
