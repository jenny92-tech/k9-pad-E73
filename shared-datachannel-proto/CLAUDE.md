# Shared 数据通道协议

> 主机与键盘之间的 BLE GATT 数据通道协议定义（`shared-datachannel-proto`）

## 地位

独立 `no_std` crate，位于 monorepo 根目录，被固件（`k9-pad-firmware`）和主机（`k9-host-lib`、`k9-host-app`）共同依赖。

## 逻辑

4 字节头（CMD+TYPE+LEN）+ payload，最大 64 字节。
Protocol version 1（`PROTOCOL_VERSION` 常量）；支持 `GetCapabilities`/`CapabilitiesResp`
命令对查询协议版本和固件版本，`DeviceCapabilities` 结构体（4 bytes wire format）。

## 约束

- `no_std` + `cfg(test)` std
- 不依赖 alloc

## 业务域清单

| 名称 | 文件 | 职责 |
|------|------|------|
| 协议定义 | `src/lib.rs` | CommandId/DataType 枚举 + PadConfig/DeviceCapabilities + parse/serialize |
