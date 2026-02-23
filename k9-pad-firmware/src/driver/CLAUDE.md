# 硬件驱动层

> 芯片级硬件抽象，直接操作寄存器和外设总线

## 地位

底层硬件驱动，被 `display/`、`settings.rs` 等上层模块引用。

## 逻辑

封装 nRF52840 芯片外设的底层访问：I2C OLED 驱动、NVMC Flash 读写。

## 约束

- `no_std`
- 所有 `unsafe` 操作必须附带 `// SAFETY:` 注释
- 不包含业务逻辑

## 业务域清单

| 名称 | 文件 | 职责 |
|------|------|------|
| OLED 驱动 | `sh1107.rs` | SH1107 I2C 显示驱动（横屏 128x64，脏页刷新，DrawTarget impl） |
| Flash KV | `flash.rs` | 通用 log-structured flash KV 存储（NVMC 单页实现） |
