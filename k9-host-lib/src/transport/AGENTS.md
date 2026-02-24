# transport
> 传输层抽象 — 定义 Transport trait 并提供 BLE 和 USB 两种实现

## 地位

`k9-host-lib` 的底层通信模块。`K9Client` 通过 `Transport` trait 发送/接收原始字节，不关心底层传输方式。本模块提供 trait 定义和两种具体实现。

## 逻辑

- `mod.rs`：定义 `Transport` trait（send, receive, disconnect, is_connected）和 `TransportError` 枚举，feature-gate 导入 ble/usb 子模块，提供 `AnyTransport` enum dispatch（需同时启用 ble+usb feature）
- `ble.rs`：`BleTransport` — 使用 `bluest` 库，先尝试已连接的外设（macOS paired keyboards），再扫描发现设备，通过 GATT characteristic 读写数据，后台 task 缓冲 TX notification
- `usb.rs`：`UsbTransport` — 使用 `hidapi` 库，通过 USB VID/PID + usage_page=0xFF61 自动检测 K9-Pad data channel 设备。`auto_connect()` 优先匹配 usage_page，回退到 VID/PID 匹配 + PING/PONG 探测。所有阻塞式 HID I/O 通过 `spawn_blocking` 运行，不阻塞 tokio runtime。每次 send 发送 65 字节（1 字节 report ID + 64 字节数据），每次 receive 读取 64 字节并从 header 解析实际长度

## 约束

- 所有 Transport 实现必须 `Send + Sync`（用于跨 async task）
- BLE 通过 `bluest` 库，仅支持其兼容的平台（macOS CoreBluetooth、Linux BlueZ、Windows WinRT）
- USB 使用阻塞式 `hidapi` 读写，通过 `Arc<std::sync::Mutex>` + `tokio::task::spawn_blocking` 避免阻塞 tokio runtime
- BLE notification 缓冲区上限 32 条，满后丢弃新消息
- USB Raw HID 每报文固定 64 字节，通过 `shared-datachannel-proto` header 解析实际包长

## 业务域清单

| 名称 | 文件/子目录 | 职责 |
|------|------------|------|
| Trait 定义 | `mod.rs` | Transport trait + TransportError + AnyTransport enum dispatch + 子模块 feature-gate |
| BLE 传输 | `ble.rs` | BleTransport — 蓝牙设备发现 + GATT I/O + notification 缓冲 |
| USB 传输 | `usb.rs` | UsbTransport — USB Raw HID 连接 + usage_page 过滤 + 非阻塞报文读写 |
