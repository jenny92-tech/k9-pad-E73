# 构建工具

> DFU 固件包生成、CRC 补丁和 Layer 数据生成工具

## 地位

构建流水线辅助脚本，不参与固件编译。

## 逻辑

`gen_dfu_pkg.py` 生成 DFU zip 包；`gen_layer_data.py` 生成 Layer 相关 C 数据块。

## 约束

- 纯 Python 3，无第三方依赖

## 业务域清单

| 名称 | 文件 | 职责 |
|------|------|------|
| DFU 包生成 | `gen_dfu_pkg.py` | Adafruit SDK 11 格式 DFU zip |
| Layer 数据生成 | `gen_layer_data.py` | 生成 WouoUI_k9pad.c 中的图标+文本查找表 |
