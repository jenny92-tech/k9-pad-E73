// INPUT:  k9-host-lib (K9Client, BleTransport, UsbTransport, Transport trait)
// OUTPUT: CLI example that validates BLE/USB communication with K9-Pad keyboard
// POS:    Developer tool for verifying host-keyboard data channel link

use std::time::{Duration, Instant};

use clap::Parser;
use k9_host_lib::{K9Client, Transport};
use k9_datachannel_proto::function_bits;

#[derive(Parser)]
#[command(name = "test_connection", about = "K9-Pad communication test")]
struct Cli {
    /// Use BLE transport (default)
    #[arg(long, group = "transport")]
    ble: bool,

    /// Use USB transport
    #[arg(long, group = "transport")]
    usb: bool,

    /// USB serial port path (auto-detect if omitted)
    #[arg(long, requires = "usb")]
    port: Option<String>,
}

#[tokio::main]
async fn main() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    let cli = Cli::parse();

    // Default to BLE if neither flag is specified
    if cli.usb {
        run_usb(cli.port).await;
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
async fn run_usb(port: Option<String>) {
    use k9_host_lib::UsbTransport;

    println!("=== K9-Pad USB Connection Test ===\n");

    // List available ports for reference
    let ports = UsbTransport::list_ports();
    if !ports.is_empty() {
        println!("Available serial ports: {}", ports.join(", "));
    }

    // --- Connect ---
    print_step("Connecting via USB...");
    let start = Instant::now();
    let transport = match port {
        Some(ref p) => {
            println!("  (using port: {p})");
            UsbTransport::connect(p, 115200)
        }
        None => {
            println!("  (auto-detecting K9-Pad...)");
            UsbTransport::auto_connect()
        }
    };
    let transport = match transport {
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

#[cfg(not(feature = "usb"))]
async fn run_usb(_port: Option<String>) {
    eprintln!("USB feature is not enabled. Rebuild with `--features usb`.");
}

async fn run_test_sequence<T: Transport>(client: &K9Client<T>) {
    println!();

    // 1. GetCapabilities (probe firmware version & supported commands)
    print_step("get_capabilities()");
    let start = Instant::now();
    match client.get_capabilities().await {
        Ok(caps) => {
            print_ok(start.elapsed());
            println!(
                "  protocol: v{}, firmware: {}.{}.{}, hw: v{}, slots: {}",
                caps.protocol_version,
                caps.firmware_major,
                caps.firmware_minor,
                caps.firmware_patch,
                caps.hw_version,
                caps.max_slots
            );
            println!(
                "  supported_cmds: 0x{:04X}, supported_types: 0x{:04X}",
                caps.supported_cmds, caps.supported_types
            );
        }
        Err(e) => {
            print_fail(&format!("{e}"));
            println!("  (legacy firmware without capability negotiation?)");
        }
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
