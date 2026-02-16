# 固件完整性校验方案调研（备用）

> 状态：**暂存**。如 DFU 问题能解决则删除此文档。

## 背景

DFU 刷写中断可能导致固件损坏。调研了两个方向：
1. 轻量哈希算法替代 1KB CRC32 表
2. 三层启动架构（Bootloader → FirmwareCheck → Firmware）

## 结论：Bootloader 已有防砖机制

Adafruit Bootloader 0.10.0 源码确认：

1. DFU 擦除后**立即**写 `bank_0 = BANK_INVALID_APP`（0xFF）
2. 全部数据写入 + CRC 验证通过**之后**才写 `bank_0 = BANK_VALID_APP`（0x01）
3. 如果 DFU 中断，下次启动 `bootloader_app_is_valid()` 返回 false → 自动进入 DFU

**源码路径**（Adafruit_nRF52_Bootloader）：
- `lib/sdk11/.../dfu_single_bank.c` — DFU 状态机
- `lib/sdk11/.../bootloader.c` — bank_0 状态管理
- `lib/sdk11/.../bootloader_types.h` — `BANK_VALID_APP=0x01, BANK_INVALID_APP=0xFF`

---

## 调研 1：轻量哈希算法

| 算法 | 代码 | 表大小 | 400KB 耗时 | 推荐 |
|------|------|--------|-----------|------|
| CRC32 1KB表 (当前) | ~100B | 1024B | ~51ms | 当前 |
| **CRC32 半字节表** | ~120B | **64B** | ~90ms | **首选替换** |
| CRC32 无表 | ~80B | 0B | ~256ms | 最省空间 |
| Fletcher-32 | ~50B | 0B | ~26ms | 最快 |
| CC310 SHA-256 | ~3-5KB | 0B | ~15ms | 硬件加速 |

CRC32 半字节表实现（16 entries = 64 bytes）：
```rust
const CRC32_NIBBLE: [u32; 16] = [
    0x00000000, 0x1DB71064, 0x3B6E20C8, 0x26D930AC,
    0x76DC4190, 0x6B6B51F4, 0x4DB26158, 0x5005713C,
    0xEDB88320, 0xF00F9344, 0xD6D6A3E8, 0xCB61B38C,
    0x9B64C2B0, 0x86D3D2D4, 0xA00AE278, 0xBDBDF21C,
];

fn crc32_nibble(data: &[u8]) -> u32 {
    let mut crc = 0xFFFF_FFFFu32;
    for &b in data {
        crc = CRC32_NIBBLE[((crc ^ b as u32) & 0x0F) as usize] ^ (crc >> 4);
        crc = CRC32_NIBBLE[((crc ^ (b as u32 >> 4)) & 0x0F) as usize] ^ (crc >> 4);
    }
    crc ^ 0xFFFF_FFFF
}
```

---

## 调研 2：三层启动架构

### 方案
```
0x00000-0x26000  MBR + SoftDevice (不变)
0x26000-0x27000  FirmwareCheck (4KB, 固定, 不被 DFU 覆盖)
0x27000-0xF4000  Main Firmware (820KB, DFU 目标)
0xF4000+         Bootloader (不变)
```

### 可行性

| 项 | 结论 |
|----|------|
| 4KB 够用 | 是，最小 ~900B，含 vector table ~3KB |
| VTOR 跳转 | 可行：`SCB.VTOR=0x27000` + MSP + BX |
| Cargo 双二进制 | workspace 两个 crate，各自 memory.x |
| **DFU 地址可改？** | **不可** — 硬编码 `SD_SIZE_GET()=0x26000` |
| **SDK 11 有地址字段？** | **无** |
| **ACL 写保护？** | 每次复位清零，DFU 期间无效 |

### 致命障碍
DFU 永远从 0x26000 擦写，会覆盖 FirmwareCheck。除非改 Bootloader 源码。

### 替代方案
- 把 FirmwareCheck 打包在每次 DFU 中（多 4KB）
- 改 Bootloader 的 `DFU_BANK_0_REGION_START`（不推荐）

---

## nRF52840 硬件能力备忘

- **CC310 CryptoCell**: SHA-256 硬件加速 ~15ms/400KB，需 `nrf_cc310_bl` FFI
- **ACL**: 8 个区域，4KB 粒度，write-once per reset，不持久
- **无硬件 CRC**：ARMv7E-M 无 CRC 指令

---

## 跳转代码参考（FirmwareCheck → Firmware）

```rust
const APP_ADDR: u32 = 0x0002_7000;

unsafe fn jump_to_app() -> ! {
    let vt = APP_ADDR as *const u32;
    let sp = core::ptr::read_volatile(vt);
    let reset = core::ptr::read_volatile(vt.offset(1));
    cortex_m::interrupt::disable();
    (*cortex_m::peripheral::SCB::PTR).vtor.write(APP_ADDR);
    core::arch::asm!(
        "msr MSP, {sp}",
        "bx {reset}",
        sp = in(reg) sp,
        reset = in(reg) reset,
        options(noreturn)
    );
}
```

注意：有 SoftDevice 时应用 `sd_softdevice_vector_table_base_set(0x27000)` 替代直接写 VTOR。
