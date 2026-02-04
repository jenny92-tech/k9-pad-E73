# k9-pad-E73 调试指南

## 硬件准备
- E73-2G4M08S1C 模块（nRF52840）
- J-Link 调试器
- SWD 接线：SWDIO、SWCLK、GND、VCC

## 当前固件状态

### 已完成配置
| 组件 | 状态 | 备注 |
|------|------|------|
| 键盘矩阵 | ✅ | 3x4=12位置，使用10个（SW1-9 + U2） |
| BLE | ✅ | nRF52840 BLE 已启用 |
| 编码器 | ✅ | 丰实 E8A1-4C40-9B15，A=P0.20, B=P0.18 |
| OLED 驱动 | 🟡 | SH1107 64x128 代码就绪，待接入 |

### 矩阵布局
```
Row0: [_, _, U2, SW1]
Row1: [SW2, SW3, SW4, SW5]
Row2: [SW6, SW7, SW8, SW9]
```

## 烧录步骤

### 1. 连接 J-Link
接线对照：
| J-Link | E73 模块 |
|--------|----------|
| SWDIO | SWDIO (P0.24) |
| SWCLK | SWCLCK (P0.25) |
| GND | GND |
| VTref | VCC (3.3V) |

### 2. 烧录命令
```bash
# 在 project 目录下
probe-rs run --chip nRF52840_xxAA target/thumbv7em-none-eabihf/release/k9-pad-e73
```

或使用 cargo runner：
```bash
cargo run --release
```

### 3. 查看日志
```bash
# 另一个终端
defmt-print -e target/thumbv7em-none-eabihf/release/k9-pad-e73
```

## 首次测试 checklist

- [ ] 按键能触发（10个键都要测）
- [ ] BLE 能被电脑/手机搜索到
- [ ] 能配对并输入字符
- [ ] 编码器旋转有反应

## OLED 屏幕启用（可选）
当前显示代码已写在 `src/main.rs` 的 `run_display()` 函数里，但需要手动初始化键盘才能接入。

后续如需启用，替换 `main.rs` 为手动初始化版本即可。

## 引脚对照表

来自 `keyboard.toml`：
```toml
row_pins = ["P1_11", "P1_10", "P0_03"]
col_pins = ["P1_13", "P0_02", "P0_05", "P0_07"]
```

编码器：
- A 相: P0.20 (网表中的 NF2)
- B 相: P0.18 (网表中的 NF1)

OLED (SH1107 64x128)：
- SDA: P0.08
- SCL: P1.09
- RESET: P0.06

## Vial 配置
烧录后可通过 Vial 软件配置 keymap：
1. 打开 https://vial.rocks 或下载 Vial 桌面版
2. 键盘连接后自动识别
3. 可视化配置按键功能

## 文件位置
- 固件源码: `/Users/mobot/clawd/projects/k9-pad-e73/`
- 编译输出: `target/thumbv7em-none-eabihf/release/k9-pad-e73`
- 网表备份: `schematic.net`
