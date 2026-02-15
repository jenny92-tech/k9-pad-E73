# K9-Pad E73 Build System
#
# 构建:
#   make firmware     构建固件（编译 → 导出 bin + hex）
#   make dfu          打包 BLE OTA 升级包（zip）
#   make uf2          生成 USB UF2 文件
#   make all          构建全部（固件 + DFU 包 + UF2）
#
# 烧录:
#   make flash        SWD 烧录（日常开发）
#   make flash-init   SWD 首次烧录（擦除 → REGOUT0 → Bootloader → 固件 → Settings）
#   make flash-rescue SWD 救砖（connect-under-reset）
#
# 其他:
#   make build        仅编译（ELF）
#   make clean        清理构建产物
#   make help         显示帮助

# ── 工具链 ────────────────────────────────────────────────
OBJCOPY    := arm-none-eabi-objcopy
PROBE      := probe-rs
CHIP       := nRF52840_xxAA
PYTHON     := python3

# ── 路径 ──────────────────────────────────────────────────
TARGET     := thumbv7em-none-eabihf
ELF        := target/$(TARGET)/release/k9-pad-e73
BIN        := target/k9-pad-e73.bin
HEX        := target/k9-pad-e73.hex
UF2        := target/k9-pad-e73.uf2
DFU_ZIP    := target/k9-pad-e73-dfu.zip

TOOLS      := tools
GEN_DFU    := $(TOOLS)/gen_dfu_pkg.py
SETTINGS   := $(TOOLS)/bootloader_settings_v2.hex
REGOUT0    := $(TOOLS)/regout0_3v3.hex
# Bootloader hex: 从 GitHub Releases 下载后放到 tools/ 目录
# https://github.com/adafruit/Adafruit_nRF52_Bootloader/releases/tag/0.10.0
BOOTLOADER := $(TOOLS)/nice_nano_bootloader-0.10.0_s140_6.1.1.hex

# ── 默认目标 ──────────────────────────────────────────────
.PHONY: all firmware dfu uf2 build bin hex flash flash-init flash-rescue reset clean help

all: firmware dfu uf2

# ── 固件构建 ──────────────────────────────────────────────
# 编译 + 导出 bin/hex
firmware: $(BIN) $(HEX)
	@echo "──────────────────────────────────────────"
	@echo "固件已构建:"
	@echo "  ELF: $(ELF)"
	@echo "  BIN: $(BIN)"
	@echo "  HEX: $(HEX)"
	@echo "──────────────────────────────────────────"

build:
	cargo build --release

$(ELF): build

$(BIN): $(ELF)
	$(OBJCOPY) -O binary $(ELF) $(BIN)

$(HEX): $(ELF)
	$(OBJCOPY) -O ihex $(ELF) $(HEX)

# ── DFU 包 ────────────────────────────────────────────────
# 将 bin 打包为 BLE OTA 升级 zip
dfu: $(DFU_ZIP)
	@echo "──────────────────────────────────────────"
	@echo "DFU 包已生成: $(DFU_ZIP)"
	@echo "用法: 菜单 → Settings → DFU Mode, 手机 nRF Connect 上传"
	@echo "──────────────────────────────────────────"

$(DFU_ZIP): $(BIN)
	$(PYTHON) $(GEN_DFU) $(BIN) $(DFU_ZIP)

# ── UF2 ──────────────────────────────────────────────────
# 将 hex 转换为 USB UF2 文件
uf2: $(UF2)
	@echo "──────────────────────────────────────────"
	@echo "UF2 已生成: $(UF2)"
	@echo "──────────────────────────────────────────"

$(UF2): $(HEX)
	python3 -m uf2conv $(HEX) -c -f 0xADA52840 -o $(UF2) || \
	cargo hex-to-uf2 --input-path $(HEX) --output-path $(UF2) --family nrf52840

# ── 单步目标（调试用）───────────────────────────────────
bin: $(BIN)

hex: $(HEX)

# ── SWD 烧录（日常开发）──────────────────────────────────
flash: build
	$(PROBE) download --chip $(CHIP) $(ELF)
	$(PROBE) reset --chip $(CHIP)

# ── SWD 首次烧录（全套初始化）────────────────────────────
flash-init: build
	@test -f $(BOOTLOADER) || (echo "ERROR: 未找到 $(BOOTLOADER)" && \
		echo "请从 https://github.com/adafruit/Adafruit_nRF52_Bootloader/releases/tag/0.10.0 下载" && exit 1)
	@echo "=== 1/6 全片擦除 ==="
	$(PROBE) erase --chip $(CHIP) --allow-erase-all
	@echo "=== 2/6 恢复 REGOUT0 = 3.3V ==="
	$(PROBE) download --binary-format iHex --chip $(CHIP) $(REGOUT0)
	@echo "=== 3/6 刷入 Bootloader + S140 ==="
	$(PROBE) download --binary-format iHex --chip $(CHIP) $(BOOTLOADER)
	@echo "=== 4/6 刷入应用固件 ==="
	$(PROBE) download --chip $(CHIP) $(ELF)
	@echo "=== 5/6 写入 Bootloader Settings ==="
	$(PROBE) download --binary-format iHex --chip $(CHIP) $(SETTINGS)
	@echo "=== 6/6 重启 ==="
	$(PROBE) reset --chip $(CHIP)
	@echo "=== 首次烧录完成 ==="

# ── SWD 救砖 ─────────────────────────────────────────────
flash-rescue: build
	$(PROBE) erase --connect-under-reset --chip $(CHIP) --allow-erase-all
	$(PROBE) download --binary-format iHex --chip $(CHIP) $(REGOUT0)
	$(PROBE) download --binary-format iHex --chip $(CHIP) $(BOOTLOADER)
	$(PROBE) download --chip $(CHIP) $(ELF)
	$(PROBE) download --binary-format iHex --chip $(CHIP) $(SETTINGS)
	$(PROBE) reset --chip $(CHIP)

reset:
	$(PROBE) reset --chip $(CHIP)

clean:
	@# 清理本项目 + RMK 依赖的编译缓存（含 proc-macro）
	rm -rf target/$(TARGET)/release/.fingerprint/k9-pad-e73-* \
	       target/$(TARGET)/release/.fingerprint/rmk-* \
	       target/$(TARGET)/release/deps/librmk-* \
	       target/$(TARGET)/release/deps/k9_pad_e73-* \
	       target/release/.fingerprint/rmk-macro-* \
	       target/release/deps/librmk_macro-*
	rm -f $(BIN) $(HEX) $(UF2) $(DFU_ZIP)

help:
	@echo "K9-Pad E73 构建系统"
	@echo ""
	@echo "构建:"
	@echo "  make firmware     构建固件（编译 → 导出 bin + hex）"
	@echo "  make dfu          打包 BLE OTA 升级包（zip）"
	@echo "  make uf2          生成 USB UF2 文件"
	@echo "  make all          构建全部（固件 + DFU 包 + UF2）"
	@echo ""
	@echo "烧录:"
	@echo "  make flash        SWD 烧录（日常开发）"
	@echo "  make flash-init   SWD 首次烧录（全套初始化）"
	@echo "  make flash-rescue SWD 救砖（connect-under-reset）"
	@echo ""
	@echo "其他:"
	@echo "  make build        仅编译（ELF）"
	@echo "  make clean        清理构建产物"
