# 数据通道

> BLE/USB 数据通道协议解析与命令分发

## 地位

固件与主机通信的桥梁，接收主机推送的显示数据和控制命令，
分发到显示循环；同时将菜单配置变化上报主机。

## 逻辑

`run_data_channel()` 异步任务同时监听 RMK 的 DATA_CHANNEL_RX（主机数据）
和 DATA_CHANNEL_CONFIG watch（菜单配置变化），
通过 `parse_display_packet()` / `handle_control_packet()` 解析协议包。

## 约束

- 依赖 `k9_datachannel_proto`（共享协议 crate）
- `run_data_channel()` 任务用 `#[cfg(not(test))]` 隔离（依赖 RMK）
- `parse.rs` 中的解析函数可在 host 端测试

## 业务域清单

| 名称 | 文件 | 职责 |
|------|------|------|
| 模块入口 | `mod.rs` | 类型定义（DisplayCommand, DisplayDataCache）+ 通道 statics + re-export |
| 协议解析 | `parse.rs` | parse_display_packet(), handle_control_packet() |
| 主任务 | `task.rs` | run_data_channel() 异步任务（桥接 RMK 收发） |
