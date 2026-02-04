# 蓝牙键盘电路 - 器件清单与网络连接

> 基于网表文件 `Netlist_Schematic1_2026-02-04.enet` 整理

---

## 一、核心器件清单

| 位号 | 型号 | 功能描述 | 封装 |
|------|------|----------|------|
| **U1** | E73-2G4M08S1C | nRF52840 蓝牙模块 (主控) | WIRELM-SMD |
| **U3** | ESDR0502NMUTAG | USB ESD保护芯片 | UDFN-6 |
| **U5** | XC6220B331MR-G | 3.3V LDO稳压器 (1A) | SOT-25 |
| **U6** | TP4054-42-SOT25R | 锂电池充电管理芯片 | SOT-23-5 |
| **U8** | TPS61040DBVR | 升压DC-DC (输出28V) | SOT-23-5 |
| **U9** | SH1107-64128-H13 | 1.3寸 OLED屏 (I2C) | 13PIN |
| **USB1** | TYPE-C16PIN | USB Type-C 接口 | SMD |
| **X1** | 32.768kHz晶振 | RTC时钟晶振 | SMD3215-2P |

---

## 二、电源网络

```
VBUS (USB 5V)
├─ USB1.A4B9/B4A9 ─── $1N39 ─┬─ F1 保险丝 ─── VBUS
│                            └─ C1 (10uF) ─── GND
│
├─ U3.VBUS ($1N39) ─── ESD保护
│
├─ F1 ─── VBUS ─┬─ U6.VCC (充电IC)
│               ├─ Q1.G (PMOS栅极)
│               ├─ R3 (100kΩ) ─── GND
│               ├─ D11.A ─── $1N50 (LDO输入)
│               └─ C9/C18 (滤波)
│
└─ U1.VBS (Pin27) ─── VBUS

VBAT (电池电压)
├─ U6.BAT (充电输出)
├─ Q1.D (PMOS漏极) ─── $1N50
├─ SW11.1 (电源开关)
├─ R8 (820kΩ) ─── POWER_PIN (ADC检测)
└─ C8 (10uF) ─── GND

$1N50 (LDO输入)
├─ D11.K (肖特基续流)
├─ Q1.S (PMOS源极)
└─ U5.VIN/CE ─── 输出 NRF_VDD (3.3V)

NRF_VDD (3.3V 主电源)
├─ U1.VCC (Pin19)
├─ U1.VDH (Pin23)
├─ U5.VOUT
├─ U8.VIN/EN (升压使能)
├─ U9.IM1 (OLED模式)
├─ J2.1 (SWD VCC)
├─ L1.1 ─── 升压电感
└─ C2/C5/C6/C7/C19 (滤波电容)

VCC9V (升压输出 ~9V)
├─ U8.SW ─┬─ L1.2
│         └─ D12.A ─── VCC9V
├─ R5/R6 (反馈分压) ─── $1N204 ─── U8.FB
├─ R7 (47kΩ) ─── $1N168
├─ Q2.D (PMOS) ─── OLED_VPP
└─ C10/C11 (滤波)
```

---

## 三、键盘矩阵 (3行×4列)

```
行线 (ROW) - 输出扫描:
  ROW0 ── U1.P1.11 (Pin1)
  ROW1 ── U1.P1.10 (Pin2)
  ROW2 ── U1.P0.03 (Pin3)

列线 (COL) - 输入读取:
  COL0 ── U1.P1.13 (Pin6)
  COL1 ── U1.AI0   (Pin7)
  COL2 ── U1.AI5   (Pin8)
  COL3 ── U1.AI7   (Pin9)
```

### 按键与二极管连接

| 按键 | Pin1 (二极管阳极) | Pin2 (列线) |
|------|-------------------|-------------|
| SW1 | D1.A → ROW0 | COL3 |
| SW2 | D3.A → ROW1 | COL0 |
| SW3 | D4.A → ROW1 | COL1 |
| SW4 | D5.A → ROW1 | COL2 |
| SW5 | D6.A → ROW1 | COL3 |
| SW6 | D7.A → ROW2 | COL0 |
| SW7 | D8.A → ROW2 | COL1 |
| SW8 | D9.A → ROW2 | COL2 |
| SW9 | D10.A → ROW2 | COL3 |
| U2 | D2.A → ROW0 | COL2 |

---

## 四、USB接口

```
USB1 Type-C:
├─ DP1/DP2 (A6/B6) ─── USB_DP ─┬─ U1.D+ (Pin31)
│                               └─ U3.D+ (ESD保护)
├─ DN1/DN2 (A7/B7) ─── USB_DM ─┬─ U1.D- (Pin29)
│                               └─ U3.D- (ESD保护)
├─ CC1 (A5) ─── $1N36 ─── R2 (5.1kΩ) ─── GND
├─ CC2 (B5) ─── $1N42 ─── R1 (5.1kΩ) ─── GND
├─ VBUS (A4B9/B4A9) ─── $1N39
└─ GND/SHELL ─── GND
```

---

## 五、OLED显示屏 (I2C)

```
U9 OLED (SSD1312):
├─ D0 (Pin10) ─── OLED_SCL ─── U1.P1.09 (Pin17)
├─ D1 (Pin11) ─── OLED_SDA ─── U1.P0.08 (Pin16)
├─ RES# (Pin8) ─── OLED_RES ─── U1.P0.06 (Pin14)
├─ CS# (Pin7) ─── GND (固定选中)
├─ A0 (Pin9) ─── GND (I2C模式)
├─ VDD (Pin4) ─── OLED_VDD ─┬─ C12/C13 ─── GND
├─ VPP (Pin2) ─── OLED_VPP ─┬─ Q2.S (电源开关)
│                            └─ C16/C17 ─── GND
├─ VCOMH (Pin3) ─── OLED_VCOMH ─── C15 ─── GND
├─ IREF (Pin6) ─── OLED_IREF ─── R11 (560kΩ) ─── GND
├─ IM1 (Pin5) ─── NRF_VDD
└─ VSS (Pin12) ─── GND
```

### OLED电源开关电路

```
OLED_SWITCH ─── U1.AI3 (Pin15)
     │
     ├─── R9 (47kΩ) ─── GND
     └─── Q3.G (NMOS)
              │
              Q3.D ─── $1N168 ─── Q2.G (PMOS)
              Q3.S ─── GND
```

---

## 六、充电电路

```
U6 TP4054:
├─ VCC (Pin4) ─── VBUS
├─ BAT (Pin3) ─── VBAT
├─ GND (Pin2) ─── GND
├─ CHRG (Pin1) ─── CHRG_DET ─── U1.P0.07 (Pin22)
└─ PROG (Pin5) ─── $1N83 ─── R4 (10kΩ) ─── GND
```

---

## 七、外部接口

### J1 旋钮/编码器接口 (SH1.0-4P)

```
├─ Pin1 ─── EXT_A ─── U1.P0.13 (Pin33)
├─ Pin2 ─── EXT_SW ─── U1.P0.22 (Pin34)
├─ Pin3 ─── GND
└─ Pin4 ─── EXT_B ─── U1.P0.24 (Pin35)
```

### U4 TTC编码器

```
├─ Pin1/2/4 ─── GND
├─ Pin3 ─── TTC_A ─── U1.NF2 (Pin43)
└─ Pin5 ─── TTC_B ─── U1.NF1 (Pin41)
```

### J2 SWD调试接口

```
├─ Pin1 ─── NRF_VDD (3.3V)
├─ Pin2 ─── SWDIO ─── U1.SWD (Pin37)
├─ Pin3 ─── SWCLK ─── U1.SWC (Pin39)
└─ Pin4 ─── GND
```

### U7 电池接口 (SH1.0-2P)

```
├─ Pin1 ─── $1N80 ─── SW11.2 (电源开关)
└─ Pin2/3/4 ─── GND
```

### SW10 复位按键

```
├─ Pin1/2 ─── $1N65 ─── U1.RST (Pin26)
└─ Pin3/4 ─── GND
```

### SW11 电源开关 (SPDT)

```
├─ Pin1 ─── VBAT
├─ Pin2 ─── $1N80 ─── U7.1 (电池)
├─ Pin3 ─── NC
└─ Pin4 ─── GND
```

---

## 八、晶振电路

```
X1 32.768kHz:
├─ OSC1 ─── $1N51 ─┬─ U1.XL1 (Pin11)
│                   └─ C3 (12pF) ─── GND
└─ OSC2 ─── $1N55 ─┬─ U1.XL2 (Pin13)
                    └─ C4 (12pF) ─── GND
```

---

## 九、无源器件清单

### 电阻

| 位号 | 值 | 网络连接 | 用途 |
|------|-----|----------|------|
| R1 | 5.1kΩ | GND ─ $1N42 (CC2) | USB CC下拉 |
| R2 | 5.1kΩ | GND ─ $1N36 (CC1) | USB CC下拉 |
| R3 | 100kΩ | GND ─ VBUS | VBUS分压 |
| R4 | 10kΩ | GND ─ $1N83 | 充电电流设置 |
| R5 | 1.2MΩ | $1N204 ─ VCC9V | 反馈分压上 |
| R6 | 180kΩ | GND ─ $1N204 | 反馈分压下 |
| R7 | 47kΩ | VCC9V ─ $1N168 | PMOS栅极偏置 |
| R8 | 820kΩ | POWER_PIN ─ VBAT | 电池电压检测 |
| R9 | 47kΩ | OLED_SWITCH ─ GND | NMOS下拉 |
| R10 | 2MΩ | GND ─ POWER_PIN | ADC分压 |
| R11 | 560kΩ | GND ─ OLED_IREF | OLED电流设置 |

### 电容

| 位号 | 值 | 网络连接 | 用途 |
|------|-----|----------|------|
| C1 | 10uF | GND ─ $1N39 | USB输入滤波 |
| C2 | 10uF | GND ─ NRF_VDD | 主电源滤波 |
| C3 | 12pF | GND ─ $1N51 | 晶振负载 |
| C4 | 12pF | GND ─ $1N55 | 晶振负载 |
| C5 | 100nF | GND ─ NRF_VDD | 电源去耦 |
| C6 | 100nF | GND ─ NRF_VDD | 电源去耦 |
| C7 | 10uF | NRF_VDD ─ GND | 电源滤波 |
| C8 | 10uF | VBAT ─ GND | 电池滤波 |
| C9 | 4.7uF | GND ─ VBUS | VBUS滤波 |
| C10 | 22pF | VCC9V ─ $1N204 | 反馈补偿 |
| C11 | 1uF | GND ─ VCC9V | 升压输出 |
| C12 | 100nF | OLED_VDD ─ GND | OLED去耦 |
| C13 | 4.7uF | GND ─ OLED_VDD | OLED滤波 |
| C14 | 100nF | GND ─ POWER_PIN | ADC滤波 |
| C15 | 4.7uF | OLED_VCOMH ─ GND | VCOMH滤波 |
| C16 | 4.7uF | GND ─ OLED_VPP | VPP滤波 |
| C17 | 100nF | OLED_VPP ─ GND | VPP去耦 |
| C18 | 100nF | GND ─ VBUS | VBUS去耦 |
| C19 | 100nF | NRF_VDD ─ GND | 电源去耦 |

### 其他

| 位号 | 型号/值 | 网络连接 | 用途 |
|------|---------|----------|------|
| L1 | 10uH | NRF_VDD ─ $1N200 | 升压电感 |
| F1 | 1.1A保险丝 | $1N39 ─ VBUS | 过流保护 |
| D1-D10 | 1N4148WS | 键盘矩阵 | 防反流二极管 |
| D11 | PMEG2010EJ | VBUS ─ $1N50 | 肖特基续流 |
| D12 | PMEG2010EJ | $1N200 ─ VCC9V | 升压续流 |
| Q1 | AO3407 (PMOS) | G:VBUS S:$1N50 D:VBAT | 电源切换 |
| Q2 | FDN338P (PMOS) | G:$1N168 S:OLED_VPP D:VCC9V | OLED电源 |
| Q3 | FDN335N (NMOS) | G:OLED_SWITCH S:GND D:$1N168 | OLED开关驱动 |

---

## 十、nRF52840模块引脚分配总结

| 引脚 | 名称 | 网络 | 功能 |
|------|------|------|------|
| 1 | P1.11 | ROW0 | 键盘行0 |
| 2 | P1.10 | ROW1 | 键盘行1 |
| 3 | P0.03 | ROW2 | 键盘行2 |
| 4 | AI4 | NC | 未连接 |
| 5 | GND | GND | 接地 |
| 6 | P1.13 | COL0 | 键盘列0 |
| 7 | AI0 | COL1 | 键盘列1 |
| 8 | AI5 | COL2 | 键盘列2 |
| 9 | AI7 | COL3 | 键盘列3 |
| 10 | AI6 | POWER_PIN | 电池电压ADC |
| 11 | XL1 | $1N51 | 32.768k晶振 |
| 12 | P0.26 | NC | 未连接 |
| 13 | XL2 | $1N55 | 32.768k晶振 |
| 14 | P0.06 | OLED_RES | OLED复位 |
| 15 | AI3 | OLED_SWITCH | OLED电源控制 |
| 16 | P0.08 | OLED_SDA | I2C数据 |
| 17 | P1.09 | OLED_SCL | I2C时钟 |
| 18 | AI2 | NC | 未连接 |
| 19 | VCC | NRF_VDD | 3.3V电源 |
| 20 | P12 | NC | 未连接 |
| 21 | GND | GND | 接地 |
| 22 | P0.07 | CHRG_DET | 充电状态检测 |
| 23 | VDH | NRF_VDD | 3.3V电源 |
| 24 | GND | GND | 接地 |
| 25 | DCH | NC | 未连接 |
| 26 | RST | $1N65 | 复位 |
| 27 | VBS | VBUS | USB 5V |
| 28 | P15 | NC | 未连接 |
| 29 | D- | USB_DM | USB数据- |
| 30 | P17 | NC | 未连接 |
| 31 | D+ | USB_DP | USB数据+ |
| 32 | P0.20 | NC | 未连接 |
| 33 | P0.13 | EXT_A | 外部编码器A |
| 34 | P0.22 | EXT_SW | 外部按键 |
| 35 | P0.24 | EXT_B | 外部编码器B |
| 36 | P1.00 | NC | 未连接 |
| 37 | SWD | SWDIO | 调试数据 |
| 38 | P1.02 | NC | 未连接 |
| 39 | SWC | SWCLK | 调试时钟 |
| 40 | P1.04 | NC | 未连接 |
| 41 | NF1 | TTC_B | 编码器B |
| 42 | P1.06 | NC | 未连接 |
| 43 | NF2 | TTC_A | 编码器A |

---

## 十一、网络名称汇总

### 电源网络

| 网络名 | 描述 |
|--------|------|
| GND | 公共地 |
| VBUS | USB 5V (经保险丝后) |
| $1N39 | USB VBUS原始输入 |
| VBAT | 电池电压 |
| $1N50 | LDO输入 (VBUS/VBAT切换后) |
| NRF_VDD | 3.3V主电源 |
| VCC9V | 升压输出 (~9V) |
| $1N80 | 电池开关输出 |

### 信号网络

| 网络名 | 描述 |
|--------|------|
| ROW0/ROW1/ROW2 | 键盘行线 |
| COL0/COL1/COL2/COL3 | 键盘列线 |
| USB_DP/USB_DM | USB数据线 |
| $1N36/$1N42 | USB CC线 |
| OLED_SCL/OLED_SDA | I2C总线 |
| OLED_RES | OLED复位 |
| OLED_SWITCH | OLED电源控制 |
| OLED_VDD/OLED_VPP/OLED_VCOMH/OLED_IREF | OLED电源 |
| CHRG_DET | 充电状态检测 |
| POWER_PIN | 电池电压ADC |
| SWDIO/SWCLK | SWD调试 |
| EXT_A/EXT_B/EXT_SW | 外部编码器接口 |
| TTC_A/TTC_B | 板载编码器 |
| $1N65 | 复位网络 |
| $1N83 | 充电电流设置 |
| $1N168 | OLED PMOS栅极 |
| $1N200 | 升压开关节点 |
| $1N204 | 升压反馈 |
| $1N211-$1N222 | 键盘矩阵内部节点 |

---

## 设计概述

这是一个**蓝牙机械键盘**的电路设计，主要特点:

- **主控**: nRF52840蓝牙SoC (E73-2G4M08S1C模块)
- **键盘**: 3×4矩阵 + 旋转编码器 (Kailh Choc矮轴)
- **显示**: 0.77寸OLED (128×64, I2C接口)
- **电源**: 锂电池供电 + USB-C充电 (TP4054)
- **接口**: USB-C、SWD调试、外部编码器扩展
- **特性**: 电池电压监测、OLED独立电源控制、过流保护
