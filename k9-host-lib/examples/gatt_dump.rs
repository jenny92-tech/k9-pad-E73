// INPUT:  bluest (BLE adapter), tokio
// OUTPUT: GATT 数据库 dump — 列出设备全部 service/characteristic/descriptor（含属性）
// POS:    诊断工具 — 排查数据通道 TX 特性 CCCD 句柄问题（notify 订阅失败："attribute handle given was not valid"）

use bluest::Adapter;
use uuid::Uuid;

#[tokio::main]
async fn main() {
    let adapter = match Adapter::default().await {
        Some(a) => a,
        None => {
            println!("Adapter init failed");
            return;
        }
    };
    if let Err(e) = adapter.wait_available().await {
        println!("Adapter not available: {e}");
        return;
    }
    let k9 = Uuid::from_u128(0xe9dc0001_7374_7265_616d_6b3970616400);
    let devices = match adapter.connected_devices_with_services(&[k9]).await {
        Ok(d) => d,
        Err(e) => {
            println!("connected_devices_with_services failed: {e}");
            return;
        }
    };
    if devices.is_empty() {
        let all = adapter.connected_devices().await.unwrap_or_default();
        println!("No K9 device connected. All connected: {}", all.len());
        return;
    }
    for dev in devices {
        let name = dev.name().unwrap_or_else(|_| "<unnamed>".into());
        println!("== {name} ({} ==", dev.id());
        let services = match dev.discover_services().await {
            Ok(s) => s,
            Err(e) => {
                println!("  discover_services failed: {e}");
                continue;
            }
        };
        for s in services {
            println!("  Service {}", s.uuid());
            let chars = match s.discover_characteristics().await {
                Ok(c) => c,
                Err(e) => {
                    println!("    discover_characteristics failed: {e}");
                    continue;
                }
            };
            for c in chars {
                let props = c.properties().await.unwrap_or_default();
                println!("    Char {}  props={:?}", c.uuid(), props);
                let descs = match c.discover_descriptors().await {
                    Ok(d) => d,
                    Err(e) => {
                        println!("      discover_descriptors failed: {e}");
                        continue;
                    }
                };
                for d in descs {
                    println!("      Desc {} val={:?}", d.uuid(), d.value().await.ok());
                }
            }
        }
    }
}
