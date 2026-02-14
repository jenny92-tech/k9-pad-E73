# 构建工具

> DFU 固件包生成和 CRC 补丁工具

## 地位

构建流水线辅助脚本，不参与固件编译。

## 逻辑

`gen_dfu_pkg.py` 生成 DFU zip 包；`patch_crc.py` 在 bin 中补丁 CRC32。

## 约束

- 纯 Python 3，无第三方依赖

## 业务域清单

| 名称 | 文件 | 职责 |
|------|------|------|
| DFU 包生成 | `gen_dfu_pkg.py` | Adafruit SDK 11 格式 DFU zip |
| CRC 补丁 | `patch_crc.py` | 固件 bin CRC32 完整性补丁 |
