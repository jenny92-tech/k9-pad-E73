# K9-Pad E73 Build System
#
# 常用命令:
#   make dfu        一键构建 DFU 包（编译 → 导出 bin → 嵌入 CRC → 打包 zip）
#   make flash      SWD 烧录（日常开发）
#   make flash-init SWD 首次烧录（擦除 → REGOUT0 → Bootloader → 固件 → Settings）
#   make clean      清理构建产物

# ── 工具链 ────────────────────────────────────────────────
OBJCOPY    := arm-none-eabi-objcopy
PROBE      := probe-rs
CHIP       := nRF52840_xxAA
PYTHON     := python3

# ── 路径 ──────────────────────────────────────────────────
TARGET     := thumbv7em-none-eabihf
ELF        := target/$(TARGET)/release/k9-pad-e73
BIN        := target/k9-pad-e73.bin
DFU_ZIP    := target/k9-pad-e73-dfu.zip

TOOLS      := tools
GEN_DFU    := $(TOOLS)/gen_dfu_pkg.py
SETTINGS   := $(TOOLS)/bootloader_settings_v2.hex
REGOUT0    := $(TOOLS)/regout0_3v3.hex
# Bootloader hex: 从 GitHub Releases 下载后放到 tools/ 目录
# https://github.com/adafruit/Adafruit_nRF52_Bootloader/releases/tag/0.10.0
BOOTLOADER := $(TOOLS)/pca10056_bootloader-0.10.0_s140_6.1.1.hex

# ── 默认目标 ──────────────────────────────────────────────
.PHONY: all dfu build bin patch flash flash-init reset clean help

all: dfu

# ── 完整 DFU 构建流水线 ──────────────────────────────────
# cargo build → objcopy → gen DFU zip
dfu: $(DFU_ZIP)
	@echo "──────────────────────────────────────────"
	@echo "DFU 包已生成: $(DFU_ZIP)"
	@echo "用法: 菜单 → Settings → DFU Mode, 手机 nRF Connect 上传"
	@echo "──────────────────────────────────────────"

$(DFU_ZIP): $(BIN)
	$(PYTHON) $(GEN_DFU) $(BIN) $(DFU_ZIP)

$(BIN): $(ELF)
	$(OBJCOPY) -O binary $(ELF) $(BIN)

$(ELF): build

build:
	cargo build --release

# ── 单步目标（调试用）───────────────────────────────────
bin: $(BIN)

patch: $(BIN)

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
	cargo clean
	rm -f $(BIN) $(DFU_ZIP)

help:
	@echo "K9-Pad E73 构建系统"
	@echo ""
	@echo "  make dfu          构建 DFU 包（编译 + 打包 zip）"
	@echo "  make flash        SWD 烧录（日常开发）"
	@echo "  make flash-init   SWD 首次烧录（全套初始化）"
	@echo "  make flash-rescue SWD 救砖（connect-under-reset）"
	@echo "  make build        仅编译"
	@echo "  make clean        清理"
