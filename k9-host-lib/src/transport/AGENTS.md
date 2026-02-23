# transport
> 传输层抽象 — 定义 Transport trait 并提供 BLE 和 USB 两种实现

## 地位

`k9-host-lib` 的底层通信模块。`K9Client` 通过 `Transport` trait 发送/接收原始字节，不关心底层传输方式。本模块提供 trait 定义和两种具体实现。

## 逻辑

- `mod.rs`：定义 `Transport` trait（send, receive, disconnect, is_connected）和 `TransportError` 枚举，feature-gate 导入 ble/usb 子模块
- `ble.rs`：`BleTransport` — 使用 `bluest` 库，先尝试已连接的外设（macOS paired keyboards），再扫描发现设备，通过 GATT characteristic 读写数据，后台 task 缓冲 TX notification
- `usb.rs`：`UsbTransport` — 使用 `serialport` 库，通过 USB VID/PID 自动检测 K9-Pad 设备，CDC serial 收发，读取时先解析 header 再读 payload

## 约束

- 所有 Transport 实现必须 `Send + Sync`（用于跨 async task）
- BLE 通过 `bluest` 库，仅支持其兼容的平台（macOS CoreBluetooth、Linux BlueZ、Windows WinRT）
- USB 使用阻塞式 `serialport` 读写，但通过 `tokio::sync::Mutex` 包装实现 async 接口
- BLE notification 缓冲区上限 32 条，满后丢弃新消息
- USB 接收采用 header-then-payload 两段式读取，依赖 `shared-datachannel-proto` 解析 header

## 业务域清单

| 名称 | 文件/子目录 | 职责 |
|------|------------|------|
| Trait 定义 | `mod.rs` | Transport trait + TransportError + 子模块 feature-gate |
| BLE 传输 | `ble.rs` | BleTransport — 蓝牙设备发现 + GATT I/O + notification 缓冲 |
| USB 传输 | `usb.rs` | UsbTransport — USB CDC serial 连接 + 帧读取 |
