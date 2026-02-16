# K9-Pad E73 Bootloader & DFU 踩坑记录

## 目录

- [1. Flash 布局](#1-flash-布局)
- [2. Bootloader Settings 格式之坑（SDK 11 vs SDK 15）](#2-bootloader-settings-格式之坑sdk-11-vs-sdk-15)
- [3. USB Code 43 之坑（DECUSB 未接电容）](#3-usb-code-43-之坑decusb-未接电容)
- [4. BLE OTA DFU 之坑（GPREGRET 0x57 vs 0xA8）](#4-ble-ota-dfu-之坑gpregret-0x57-vs-0xa8)
- [5. 固件完整性校验（防 DFU 变砖）](#5-固件完整性校验防-dfu-变砖)
- [6. 其他踩坑](#6-其他踩坑)
- [7. 最终工作流程](#7-最终工作流程)
- [8. 关键文件与工具](#8-关键文件与工具)

---

## 1. Flash 布局

使用 Adafruit nRF52 Bootloader 0.10.0 + S140 SoftDevice 6.1.1 时的 Flash 布局：

```
地址范围                  内容                    大小
──────────────────────────────────────────────────────
0x00000000 - 0x00001000   MBR (Master Boot Record)  4KB
0x00001000 - 0x00026000   S140 SoftDevice 6.1.1     148KB
0x00026000 - 0x000F4000   Application (键盘固件)     824KB
0x000F4000 - 0x000FE000   Bootloader                40KB
0x000FE000 - 0x000FF000   MBR Params Page           4KB
0x000FF000 - 0x00100000   Bootloader Settings        4KB
```

**memory.x 配置（Bootloader 模式）：**

```
MEMORY
{
  FLASH : ORIGIN = 0x00026000, LENGTH = 824K
  RAM   : ORIGIN = 0x20000008, LENGTH = 255K
}
```

> **注意：** RAM ORIGIN 为 `0x20000008`（偏移 8 字节），是 MBR 保留的。LENGTH 为 255K。

---

## 2. Bootloader Settings 格式之坑（SDK 11 vs SDK 15）

### 问题

刷入 Adafruit Bootloader 后，应用固件不启动（OLED 不亮），Bootloader 卡在 DFU 模式不跳转到 App。

### 根因

Adafruit nRF52 Bootloader **0.10.0 使用的是 SDK 11 的 `bootloader_settings_t` 格式**，而不是 SDK 15 的 `nrf_dfu_settings_t` 格式。网上大量资料（包括 Nordic 官方文档的新版本）描述的都是 SDK 15 格式，极易搞混。

**SDK 15 格式（❌ 错误）：**

```c
// offset 0: uint32_t crc32        ← CRC of settings struct
// offset 4: uint32_t settings_version
// offset 8: uint32_t app_version
// ...
// offset 32: bank_0.bank_code
```

**SDK 11 格式（✅ 正确）：**

```c
typedef struct {
    uint16_t bank_0;          // offset 0  — 0x0001 = BANK_VALID_APP
    uint16_t bank_0_crc;      // offset 2  — 0x0000 = 跳过 CRC 校验
    uint32_t bank_0_size;     // offset 8  — App 大小(字节)
    uint32_t sd_image_size;   // offset 12
    uint32_t bl_image_size;   // offset 16
    uint32_t app_image_size;  // offset 20 — App 大小(字节)
} bootloader_settings_t;     // 共 28 字节
```

### 为什么写 SDK 15 格式会失败

Bootloader 读取 offset 0 作为 `bank_0`（uint16），期望值为 `0x0001`（BANK_VALID_APP）。但 SDK 15 格式在 offset 0 放的是 CRC32 的低 16 位（如 `0xDB17`），不等于 `0x0001`，所以 Bootloader 认为没有有效 App，永远不会跳转。

### 修复

生成正确的 SDK 11 格式 settings 写入 0xFF000：

```python
import struct
data = bytearray(28)
struct.pack_into('<H', data, 0, 0x0001)       # bank_0 = BANK_VALID_APP
struct.pack_into('<H', data, 2, 0x0000)       # bank_0_crc = 0 (跳过校验)
struct.pack_into('<I', data, 8, app_size)     # bank_0_size
struct.pack_into('<I', data, 20, app_size)    # app_image_size
```

然后转成 Intel HEX 格式烧录到 `0x000FF000`。

---

## 3. USB Code 43 之坑（DECUSB 未接电容）

### 问题

无论使用 Embassy-USB 还是 Adafruit Bootloader 的 TinyUSB，Windows 都报 **"USB 设备描述符请求失败（代码 43）"**。

### 诊断过程

编写了诊断固件读取 nRF52840 的 POWER 寄存器：

```
USBREGSTATUS = 0x00000000
├── VBUSDETECT  = false   (未检测到 USB 5V)
├── OUTPUTRDY   = false   (内部 USB 稳压器未就绪)

EVENTS_USBDETECTED = 1    ← VBUS 检测事件触发了！
EVENTS_USBPWRRDY   = 0    ← 稳压器输出永远不稳定
```

### 根因

**PCB 设计问题：E73 模块的 DECUSB 引脚（Pin 25 / DCH）标记为 NC（未连接）。**

nRF52840 内部 USB 工作原理：

```
USB 5V (VBUS pin) → 内部 USB 3.3V 稳压器 → DECUSB 引脚(输出) → USB PHY 供电
                                                    ↑
                                              需要 4.7µF 旁路电容！
```

故障链条：

1. USB 5V → nRF52840 VBUS 引脚 → **VBUS 检测成功** ✅
2. 内部 USB 稳压器启动 → 输出到 DECUSB 引脚 ✅
3. **DECUSB 无旁路电容（NC）** → 稳压器输出无法稳定 ❌
4. `EVENTS_USBPWRRDY` 永远不触发 → USB PHY 无稳定 3.3V 供电 ❌
5. D+ 上拉正常工作（由 VDD 驱动，不依赖 DECUSB）→ Windows 看到设备 ✅
6. USB PHY 收发器无法正常通信 → 描述符请求失败 → **Code 43** ❌

### 硬件修复

在 E73 模块 **Pin 25 (DCH/DECUSB)** 和 **GND** 之间飞线焊接一个 **4.7µF** 电容。

### 诊断中的其他发现

| 测试 | 结果 |
|------|------|
| `SoftwareVbusDetect` | Windows 看到设备但 Code 43 |
| `HardwareVbusDetect` | 完全无反应（等待 PWRRDY 事件永远不触发） |
| ESD 芯片 (0.3pF) | 不是问题 |
| USB-C 接线 | CC1/CC2 下拉 5.1kΩ 正确 |
| D+ 电压 | 3V（上拉正常） |

---

## 4. BLE OTA DFU 之坑（GPREGRET 0x57 vs 0xA8）

### 问题

从菜单触发 DFU 模式后，设备直接重启回正常模式，不停留在 DFU。

### 根因

代码写入 `GPREGRET = 0x57`，这是 **USB UF2 DFU** 模式。Bootloader 进入后尝试初始化 USB，但因为 DECUSB 问题 USB 失败，导致 Bootloader 放弃并启动 App。

**Adafruit Bootloader GPREGRET 值对照：**

| 值 | 含义 | 适用场景 |
|----|------|----------|
| `0xB1` | 通用 DFU（App 跳转） | 标准 DFU 入口 |
| `0x57` | USB UF2 DFU | USB 可用时 |
| `0xA8` | **BLE OTA DFU** | **USB 不可用时使用这个** |
| `0x4E` | Serial Only DFU | 串口 DFU |

### 修复

`display.rs` 中将 GPREGRET 从 `0x57` 改为 `0xA8`：

```rust
// ❌ 错误：USB UF2 DFU（因 DECUSB 问题不可用）
embassy_nrf::pac::POWER.gpregret()
    .write_value(embassy_nrf::pac::power::regs::Gpregret(0x57));

// ✅ 正确：BLE OTA DFU（跳过 USB，只用蓝牙）
embassy_nrf::pac::POWER.gpregret()
    .write_value(embassy_nrf::pac::power::regs::Gpregret(0xA8));
```

### 双击 Reset 也不进 DFU

双击 Reset 进入的也是 USB+BLE 混合 DFU 模式，USB 初始化失败可能影响整个 DFU。这个由 Bootloader 内部控制，无法从 App 端修改。建议只通过**菜单触发 BLE OTA DFU（0xA8）** 来进入 DFU 模式。

---

## 5. 固件完整性校验（防 DFU 变砖）

### 问题

BLE OTA DFU 中断（手机断连、电池没电）后，App 区域被擦除或只写了一部分。重启后 Bootloader 仍尝试跳转到损坏的固件 → 崩溃 → 无法进入 DFU → **变砖**（只能 SWD 救）。

根本原因：Adafruit Bootloader 在 DFU 期间不一定会清除 `bootloader_settings` 的 `bank_0 = BANK_VALID`，导致 Bootloader 认为 App 仍然有效。

### DFU 擦写行为

Adafruit Bootloader 的 DFU 流程：

1. **先擦除**整个 App 区域（从 0x26000 开始，按 4KB 页逐页擦）
2. 擦除完成后，**再从头顺序写入**新固件
3. 写入完成后，更新 `bootloader_settings`

所以 DFU 中断时 Flash 的状态：

| 中断时机 | Flash 内容 | 结果 |
|----------|-----------|------|
| 擦除阶段 | 全 0xFF | Vector table 无效 → CPU 死循环 |
| 写入前半段 | 前半段有数据 + 后半段 0xFF | CPU 能启动但固件不完整 |
| 写入完成但 settings 未更新 | 完整固件 | 正常（最好情况） |

### 解决方案：启动时 CRC32 自检

在固件中嵌入 CRC32 校验值。每次启动时（`pre_init`，RAM 初始化之前），重新计算 CRC32 并对比，不一致则自动进入 BLE DFU。

**编译时（`tools/patch_crc.py`）：**

```
固件 binary 中嵌入的 FIRMWARE_INTEGRITY 结构:
┌──────────────┬──────────┬──────────┬──────────────┐
│ magic_head   │ crc32    │ size     │ magic_tail   │
│ 0x4B394352   │ (计算值) │ (文件大小) │ 0x5243394B │
│ "K9CR"       │ 4 bytes  │ 4 bytes  │ "RC9K"       │
└──────────────┴──────────┴──────────┴──────────────┘
```

- CRC32 的计算范围：整个 binary，但 crc32+size 字段替换为 0x00
- 使用标准 CRC32（IEEE 802.3，多项式 0xEDB88320），与 Python `zlib.crc32` 一致

**启动时（`src/integrity.rs`，在 `pre_init` 中调用）：**

```
启动 → pre_init() → verify_firmware()
                      ├── magic 不对（Flash 被擦/损坏）→ enter_dfu_mode()
                      ├── CRC 不匹配（固件不完整）→ enter_dfu_mode()
                      ├── CRC 未 patch（0xFFFFFFFF，开发构建）→ 跳过，正常启动
                      └── CRC 匹配 → 正常启动
```

CRC32 计算 ~400KB 固件约需 30ms（64MHz Cortex-M4），启动无感知。

### 各场景行为

| 场景 | 启动行为 |
|------|---------|
| 正常固件（CRC 已 patch） | CRC 匹配 → 正常启动 |
| DFU 擦除阶段中断 | magic = 0xFFFFFFFF ≠ 0x4B394352 → 进 DFU |
| DFU 写入阶段中断 | CRC 不匹配 → 进 DFU |
| 开发构建（未 patch） | CRC = 0xFFFFFFFF → 跳过检查，正常启动 |

### 构建流程

所有步骤已集成到 `Makefile`：

```bash
make dfu    # 一键：编译 → 导出 bin → 嵌入 CRC → 打包 DFU zip
```

等价于：

```bash
cargo build --release
arm-none-eabi-objcopy -O binary target/.../k9-pad-e73 target/k9-pad-e73.bin
python3 tools/patch_crc.py target/k9-pad-e73.bin       # 嵌入 CRC32
python3 tools/gen_dfu_pkg.py target/k9-pad-e73.bin target/k9-pad-e73-dfu.zip
```

---

## 6. 其他踩坑

### 6.1 REGOUT0 每次全擦后必须恢复

nRF52840 全片擦除（`probe-rs erase --allow-erase-all`）会清除 UICR，导致 REGOUT0 恢复默认值 1.8V。E73 模块需要 3.3V。

```bash
# 写 REGOUT0 = 3.3V 的 hex 文件
probe-rs download --binary-format iHex --chip nRF52840_xxAA regout0_3v3.hex
```

`regout0_3v3.hex` 内容（UICR 0x10001304 = 0xFFFFFFFD）：

```
:020000041000EA
:0413040005FFFFFFFB
:00000001FF
```

> **不恢复 REGOUT0 的后果：** 芯片可能无法正常工作或外设异常。

### 6.2 External Crystal 配置

E73-2G4M08S1C 模块有 32MHz 高频晶振，但**没有 32.768kHz 低频晶振**。如果设置 `LfclkSource::ExternalXtal`，`embassy_nrf::init()` 会死等 LFCLK 启动而永远卡住。

```rust
// ❌ 会卡死
config.lfclk_source = embassy_nrf::config::LfclkSource::ExternalXtal;

// ✅ 使用默认配置（内部 RC 振荡器）
let p = embassy_nrf::init(embassy_nrf::config::Config::default());
```

> **注意：** RMK 框架使用默认配置，不会有这个问题。只有独立测试固件才需要注意。

### 6.3 nrf-mpsl 的 critical-section 冲突

主项目使用 `nrf-mpsl` 提供 critical-section 实现。如果编写独立测试 bin（如 USB 测试），不能同时使用 `cortex-m` 的 `critical-section-single-core` feature 和 `nrf-mpsl`。

- 主项目 bin → 用 `extern crate nrf_mpsl;`（但需要 MPSL 已初始化）
- 独立测试项目 → 用 `cortex-m` 的 `critical-section-single-core`，**不引入** nrf-sdc/nrf-mpsl

### 6.4 SWD 连接失败

固件崩溃后可能导致 SWD 连接失败。解决方法：

```bash
# 方法 1：connect-under-reset
probe-rs erase --connect-under-reset --chip nRF52840_xxAA --allow-erase-all

# 方法 2：等几秒再试（有时芯片会自行恢复）
sleep 3 && probe-rs erase --chip nRF52840_xxAA --allow-erase-all
```

---

## 7. 最终工作流程

所有构建流程已集成到项目根目录的 `Makefile` 中。

### 首次烧录（需要 SWD）

```bash
# 一键首次烧录：擦除 → REGOUT0 → Bootloader → 固件 → Settings → 重启
make flash-init

# 前提：需要将 Bootloader hex 放到 tools/ 目录
# 下载地址：https://github.com/adafruit/Adafruit_nRF52_Bootloader/releases/tag/0.10.0
# 文件名：pca10056_bootloader-0.10.0_s140_6.1.1.hex
```

### 日常 SWD 开发

```bash
make flash    # 编译 + SWD 烧录 + 重启
```

### BLE OTA 升级（无需 SWD）

```bash
make dfu      # 一键：编译 → 导出 bin → 嵌入 CRC32 → 打包 DFU zip
              # 输出: target/k9-pad-e73-dfu.zip
```

然后在设备上：
1. 菜单 → Settings → **DFU Mode**（触发 GPREGRET=0xA8 → BLE OTA DFU）
2. 手机打开 **nRF Connect**
3. 扫描 → 连接 **AdaDFU** → 右上角 DFU 图标 → 选择 `.zip` 文件 → 上传

### 救砖（SWD 连接困难时）

```bash
make flash-rescue    # connect-under-reset 方式全套重刷
```

---

## 8. 关键文件与工具

| 文件 | 说明 |
|------|------|
| `Makefile` | **构建入口**：`make dfu` / `make flash` / `make flash-init` |
| `memory.x` | Flash/RAM 布局，Bootloader 模式下 ORIGIN=0x26000 |
| `src/integrity.rs` | 固件 CRC32 自检（`pre_init` 阶段） |
| `src/display.rs` | DFU 触发代码，GPREGRET=0xA8 |
| `tools/patch_crc.py` | 编译后脚本：计算 CRC32 嵌入 binary |
| `tools/gen_dfu_pkg.py` | DFU 包生成脚本（纯 Python，SDK 11 格式） |
| `tools/bootloader_settings_v2.hex` | SDK 11 格式 Bootloader Settings |
| `tools/regout0_3v3.hex` | UICR REGOUT0 = 3.3V |
| Bootloader hex | Adafruit Bootloader + S140，[GitHub Releases](https://github.com/adafruit/Adafruit_nRF52_Bootloader/releases/tag/0.10.0) |
| `docs/netlist_v1.md` | PCB 网表（USB 线路分析参考） |
