# k9-pad-E73 (RMK firmware)

基于 **RMK** 的 nRF52840（E73-2G4M08S1C）9 键蓝牙小键盘固件工程。

> 说明：原理图里 OLED 器件用的是一个“接口一致但规格不一致”的占位符；实际屏幕为 **SH1107 64x128 I2C**。

## 硬件信息（来自 netlist）

### MCU
- E73-2G4M08S1C (nRF52840)

### 键盘矩阵
- **Rows**: P1.11, P1.10, P0.03
- **Cols**: P1.13, P0.02, P0.05, P0.07
- 二极管方向：**col -> row**（RMK 默认 col2row，无需额外配置）

### OLED（实际）
- Driver: **SH1107**
- Resolution: **64x128**
- Interface: I2C
- Pins (from netlist):
  - SDA: P0.08
  - SCL: P1.09
  - RES#: P0.06

## 参考文档
- RMK 本地编译（你发的这篇）：https://rmk.rs/docs/user_guide/create_firmware/local_compilation.html

## 本地编译（最小步骤）

```bash
rustup target add thumbv7em-none-eabihf
cargo build --release
```

> 生成 UF2 / 烧录（如你用 nice!nano bootloader）可以按 RMK 文档里 `cargo make uf2 --release` 走。

## 配置入口
- `keyboard.toml`：矩阵引脚、层数、keymap
- `src/main.rs`：RMK 宏入口（模板方式）
