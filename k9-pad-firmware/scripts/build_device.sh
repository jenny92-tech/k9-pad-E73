#!/usr/bin/env bash
# 按设备名构建独立固件包。
#
# 用法:
#   scripts/build_device.sh <id> [name-prefix]
#
# 例子:
#   scripts/build_device.sh 009            -> 设备名 "K9-Pad-009"
#   scripts/build_device.sh 12 K9-Pad      -> 设备名 "K9-Pad-12"
#   scripts/build_device.sh K9-Pad-Custom  -> 含 "-" 时按完整设备名处理
#
# 行为:临时改写 keyboard.toml 的 name/product_name -> 构建 -> 把产物复制成按设备命名
# 的文件到 target/devices/ -> **无论成功失败都还原 keyboard.toml**。
# 设备名只影响 USB/BLE 广播名,不影响键位布局哈希(存储/配对跨设备一致)。
set -euo pipefail

# 切到固件根目录(脚本在 firmware/scripts/ 下)
cd "$(dirname "$0")/.."

ID="${1:?用法: build_device.sh <id> [name-prefix]   例: build_device.sh 009}"
PREFIX="${2:-K9-Pad}"

# 含 "-" 视为完整设备名;否则用 "<prefix>-<id>"
if [[ "$ID" == *-* ]]; then
    NAME="$ID"
    SLUG="$ID"
else
    NAME="${PREFIX}-${ID}"
    SLUG="${PREFIX,,}-${ID}"   # 小写做文件名
fi

TOML="keyboard.toml"
BACKUP="$(mktemp)"
cp "$TOML" "$BACKUP"
restore() { cp "$BACKUP" "$TOML"; rm -f "$BACKUP"; }
trap restore EXIT   # 异常/中断也会还原 keyboard.toml

echo "==> 设备名: ${NAME}"

# 改写 name / product_name(macOS BSD sed)
sed -i '' -E "s/^name = \".*\"/name = \"${NAME}\"/" "$TOML"
sed -i '' -E "s/^product_name = \".*\"/product_name = \"${NAME}\"/" "$TOML"

# 构建全部产物(bin/hex/uf2/dfu)
make all

# 复制成按设备命名的产物
OUT="target/devices"
mkdir -p "$OUT"
cp target/k9-pad-e73.uf2     "${OUT}/${SLUG}.uf2"
cp target/k9-pad-e73-dfu.zip "${OUT}/${SLUG}-dfu.zip"
cp target/k9-pad-e73.hex     "${OUT}/${SLUG}.hex"

echo "──────────────────────────────────────────"
echo "✓ ${NAME} 构建完成:"
echo "  ${OUT}/${SLUG}.uf2       (拖拽烧录)"
echo "  ${OUT}/${SLUG}-dfu.zip   (BLE OTA)"
echo "  ${OUT}/${SLUG}.hex       (SWD)"
echo "  keyboard.toml 已还原"
echo "──────────────────────────────────────────"
