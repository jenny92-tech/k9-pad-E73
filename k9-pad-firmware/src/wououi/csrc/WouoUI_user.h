// INPUT:  WouoUI.h
// OUTPUT: TestUI_Init() declaration, extern page objects
// POS:    示例菜单头文件（历史遗留，K9-Pad 使用 WouoUI_k9pad.c）
#ifndef __TEST_UI_H__
#define __TEST_UI_H__

#ifdef __cplusplus
extern "C" {
#endif

#include "WouoUI.h"
void TestUI_Init(void);
extern WavePage wave_page;
#ifdef __cplusplus
}
#endif

#endif
