# 硬件驱动层

> 芯片级硬件抽象，直接操作寄存器和外设总线

## 地位

底层硬件驱动，被 `display/`、`battery.rs`、`settings.rs` 等上层模块引用。

## 逻辑

封装 nRF52840 芯片外设的底层访问：板级常量、GPIO/SAADC 寄存器操作、I2C OLED 驱动、NVMC Flash 读写。

## 约束

- `no_std`
- 所有 `unsafe` 操作必须附带 `// SAFETY:` 注释
- 不包含业务逻辑

## 业务域清单

| 名称 | 文件 | 职责 |
|------|------|------|
| 板级常量 | `board.rs` | 引脚定义、ADC 校准、显示地址、Flash 页等硬件"魔法数字"唯一来源 |
| GPIO 操作 | `gpio.rs` | 通用 GPIO 寄存器操作（参数化 port/pin）：上拉、输入/输出配置、读取 |
| SAADC 操作 | `saadc.rs` | 通用 SAADC 寄存器操作（参数化 AIN channel）：阻塞式单次采样 |
| OLED 驱动 | `sh1107.rs` | SH1107 I2C 显示驱动（横屏 128x64，脏页刷新，DrawTarget impl） |
| Flash KV | `flash.rs` | 通用 log-structured flash KV 存储（NVMC 单页实现） |
