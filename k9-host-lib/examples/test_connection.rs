// INPUT:  k9-host-lib (K9Client, BleTransport, UsbTransport, Transport trait), clap (CLI args)
// OUTPUT: CLI test harness — connects via BLE or USB, runs full command sequence, prints results
// POS:    Developer example — end-to-end validation of host-keyboard data channel communication

use std::time::{Duration, Instant};

use clap::Parser;
use k9_datachannel_proto::function_bits;
use k9_host_lib::{K9Client, Transport};

#[derive(Parser)]
#[command(name = "test_connection", about = "K9-Pad communication test")]
struct Cli {
    /// Use BLE transport (default)
    #[arg(long, group = "transport")]
    ble: bool,

    /// Use USB transport
    #[arg(long, group = "transport")]
    usb: bool,

    /// USB HID device path (auto-detect if omitted)
    #[arg(long, requires = "usb")]
    device: Option<String>,
}

#[tokio::main]
async fn main() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    let cli = Cli::parse();

    // Default to BLE if neither flag is specified
    if cli.usb {
        run_usb(cli.device).await;
    } else {
        run_ble().await;
    }
}

#[cfg(feature = "ble")]
async fn run_ble() {
    use k9_host_lib::BleTransport;

    println!("=== K9-Pad BLE Connection Test ===\n");

    // --- Connect ---
    print_step("Connecting via BLE (scanning for 10s)...");
    let start = Instant::now();
    let transport = match BleTransport::connect(Duration::from_secs(10)).await {
        Ok(t) => {
            print_ok(start.elapsed());
            t
        }
        Err(e) => {
            print_fail(&format!("{e}"));
            return;
        }
    };

    let client = K9Client::new(transport);
    run_test_sequence(&client).await;

    // --- Disconnect ---
    print_step("Disconnecting...");
    let start = Instant::now();
    match client.transport().disconnect().await {
        Ok(()) => print_ok(start.elapsed()),
        Err(e) => print_fail(&format!("{e}")),
    }

    println!("\n=== Test Complete ===");
}

#[cfg(not(feature = "ble"))]
async fn run_ble() {
    eprintln!("BLE feature is not enabled. Rebuild with `--features ble`.");
}

#[cfg(feature = "usb")]
async fn run_usb(_device: Option<String>) {
    use k9_host_lib::UsbTransport;

    println!("=== K9-Pad USB Connection Test ===\n");

    // List available HID devices for reference
    let devices = UsbTransport::list_devices();
    if !devices.is_empty() {
        println!("K9-Pad HID devices:");
        for d in &devices {
            println!(
                "  {} (VID:{:04X} PID:{:04X} usage_page:0x{:04X}) {}",
                d.path, d.vendor_id, d.product_id, d.usage_page, d.product
            );
        }
    }

    // --- Connect ---
    print_step("Connecting via USB Raw HID...");
    let start = Instant::now();
    let transport = {
        println!("  (auto-detecting K9-Pad + probing data channel interface...)");
        match UsbTransport::auto_connect().await {
            Ok(t) => t,
            Err(e) => {
                print_fail(&format!("{e}"));
                return;
            }
        }
    };
    print_ok(start.elapsed());

    let client = K9Client::new(transport);
    run_test_sequence(&client).await;

    // --- Disconnect ---
    print_step("Disconnecting...");
    let start = Instant::now();
    match client.transport().disconnect().await {
        Ok(()) => print_ok(start.elapsed()),
        Err(e) => print_fail(&format!("{e}")),
    }

    println!("\n=== Test Complete ===");
}

#[cfg(not(feature = "usb"))]
async fn run_usb(_device: Option<String>) {
    eprintln!("USB feature is not enabled. Rebuild with `--features usb`.");
}

async fn run_test_sequence<T: Transport>(client: &K9Client<T>) {
    println!();

    // 1. GetCapabilities
    print_step("get_capabilities()");
    let start = Instant::now();
    match client.get_capabilities().await {
        Ok(caps) => {
            print_ok(start.elapsed());
            println!(
                "  protocol: v{}, firmware: {}.{}.{}",
                caps.protocol_version,
                caps.firmware_major,
                caps.firmware_minor,
                caps.firmware_patch
            );
        }
        Err(e) => print_fail(&format!("{e}")),
    }

    // 2. Ping
    print_step("ping()");
    let start = Instant::now();
    match client.ping().await {
        Ok(()) => print_ok(start.elapsed()),
        Err(e) => print_fail(&format!("{e}")),
    }

    // 3. GetStatus
    print_step("get_status()");
    let start = Instant::now();
    match client.get_status().await {
        Ok(config) => {
            print_ok(start.elapsed());
            println!("  active_pad: {}", config.active_pad);
            println!(
                "  enabled_functions: 0x{:04X} ({})",
                config.enabled_functions,
                format_functions(config.enabled_functions)
            );
        }
        Err(e) => print_fail(&format!("{e}")),
    }

    // 4. Push text
    print_step("push_text(0, \"Hello\")");
    let start = Instant::now();
    match client.push_text(0, "Hello").await {
        Ok(()) => print_ok(start.elapsed()),
        Err(e) => print_fail(&format!("{e}")),
    }

    // 5. Push numeric
    print_step("push_numeric(1, 12345)");
    let start = Instant::now();
    match client.push_numeric(1, 12345).await {
        Ok(()) => print_ok(start.elapsed()),
        Err(e) => print_fail(&format!("{e}")),
    }

    // 6. Push progress
    print_step("push_progress(2, 75)");
    let start = Instant::now();
    match client.push_progress(2, 75).await {
        Ok(()) => print_ok(start.elapsed()),
        Err(e) => print_fail(&format!("{e}")),
    }

    // 7. Wait for keyboard to display
    println!("\n  Waiting 3 seconds for keyboard display...\n");
    tokio::time::sleep(Duration::from_secs(3)).await;

    // 8. Clear all slots
    for slot in 0..3u8 {
        print_step(&format!("clear_slot({slot})"));
        let start = Instant::now();
        match client.clear_slot(slot).await {
            Ok(()) => print_ok(start.elapsed()),
            Err(e) => print_fail(&format!("{e}")),
        }
    }
}

fn format_functions(bits: u16) -> String {
    let mut names = Vec::new();
    if bits & function_bits::FOLLOW_PC != 0 {
        names.push("FOLLOW_PC");
    }
    if bits & function_bits::VOLUME != 0 {
        names.push("VOLUME");
    }
    if bits & function_bits::SUBSCRIBERS != 0 {
        names.push("SUBSCRIBERS");
    }
    if bits & function_bits::TIME != 0 {
        names.push("TIME");
    }
    if bits & function_bits::AI_QUOTA != 0 {
        names.push("AI_QUOTA");
    }
    if names.is_empty() {
        "none".to_string()
    } else {
        names.join(" | ")
    }
}

fn print_step(name: &str) {
    print!("  [{name}] ... ");
}

fn print_ok(elapsed: Duration) {
    println!("OK ({:.1}ms)", elapsed.as_secs_f64() * 1000.0);
}

fn print_fail(msg: &str) {
    println!("FAIL: {msg}");
}
