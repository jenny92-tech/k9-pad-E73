// INPUT:  wououi_types.h
// OUTPUT: WouoUI_K9Pad_ShowHostDialog / GetHostDialogResult / ClearHostDialogResult 声明
// POS:    K9-Pad 主机确认弹窗导出头文件（实现在 WouoUI_k9pad.c，Rust 侧经 extern "C" 链接）
#ifndef __WOUOUI_K9PAD_H__
#define __WOUOUI_K9PAD_H__

#ifdef __cplusplus
extern "C" {
#endif

#include "wououi_types.h"

// 显示主机确认弹窗（BLE ShowDialog 命令触发，text 以 0 结尾，最长 63 字节）
void WouoUI_K9Pad_ShowHostDialog(const char* text);

// 读取主机弹窗结果 (0=未决 1=确认 2=取消)
uint8_t WouoUI_K9Pad_GetHostDialogResult(void);

// 清除主机弹窗结果
void WouoUI_K9Pad_ClearHostDialogResult(void);

#ifdef __cplusplus
}
#endif

#endif // __WOUOUI_K9PAD_H__
