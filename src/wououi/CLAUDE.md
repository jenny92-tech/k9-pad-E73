# WouoUI FFI

> WouoUI C 库的 Rust FFI 绑定层

## 地位

C 库和 Rust 显示循环之间的桥梁。

## 逻辑

`mod.rs` 封装 `extern "C"` 函数为安全 Rust API（`WouoUI` struct）

## 约束

- 所有 C 函数通过 `extern "C"` 调用
- 缓冲区 1024 字节共享

## 业务域清单

| 名称 | 文件/子目录 | 职责 |
|------|------------|------|
| Rust 绑定 | `mod.rs` | WouoUI struct + init/tick/input/get_buffer FFI |
| C 源码 | `csrc/` | WouoUI C 库源码 |
