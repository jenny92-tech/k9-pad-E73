# shared-datachannel-proto/src
> 协议源码目录 — CommandId/DataType 枚举 + 编解码 + 设备标识常量

## 地位

`shared-datachannel-proto` crate 的全部源码所在目录。`lib.rs` 为 crate 入口，定义协议枚举、结构体和编解码函数；`identifiers.rs` 提供固件和主机共用的设备标识常量。

## 逻辑

- `lib.rs`：协议核心 — `CommandId`/`DataType` 枚举、`PacketHeader` 编解码、`PadConfig`/`DeviceCapabilities` 结构体、`function_bits` 位掩码、`build_*` 系列 packet 构造函数
- `identifiers.rs`：设备标识 single source of truth — USB VID/PID、HID usage page（0xFF61）、BLE GATT service/characteristic UUID

## 约束

- `#![cfg_attr(not(test), no_std)]`，测试时切换到 std
- 不依赖 alloc，所有编解码操作在调用方提供的 `&mut [u8]` 上完成
- 最大包长 64 字节（`MAX_PACKET_SIZE`），与 BLE characteristic size 和 USB HID report 对齐

## 业务域清单

| 名称 | 文件 | 职责 |
|------|------|------|
| 协议定义 | `lib.rs` | CommandId/DataType 枚举 + PadConfig/DeviceCapabilities + parse/serialize + 单元测试 |
| 设备标识 | `identifiers.rs` | USB VID/PID、usage page、BLE UUID — 固件和主机共用的 single source of truth |
