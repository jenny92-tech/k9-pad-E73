# WouoUI C 源码

> 嵌入式 OLED 动画菜单 C 库（128x64，SSD1306 缓冲格式）

## 地位

菜单渲染核心，通过 `build.rs` 用 `arm-none-eabi-gcc` 交叉编译。

## 逻辑

`WouoUI.c`（主状态机）→ page/win（页面/弹窗）→ graph（绘图）→ port（平台接口）

## 约束

- 无标准库（`WOUOUI_EMBEDDED`）
- `wououi_types.h` 替代 `stdint`
- 禁用 `printf`

## 业务域清单

| 名称 | 文件 | 职责 |
|------|------|------|
| 主状态机 | `WouoUI.c/h` | UI 主循环、页面调度 |
| 动画引擎 | `WouoUI_anim.c/h` | 非线性插值动画 |
| 绘图层 | `WouoUI_graph.c/h` | 像素/线/矩形/文字绘制 |
| 页面系统 | `WouoUI_page.c/h` | TitlePage/ListPage/WavePage 等 |
| 弹窗系统 | `WouoUI_win.c/h` | MsgWin/ConfWin/ValWin 等 |
| 消息队列 | `WouoUI_msg.c/h` | 输入消息环形队列 |
| 字体数据 | `WouoUI_font.c/h` | ASCII 点阵字体 |
| 用户配置 | `WouoUI_user.c/h` | 示例菜单树（未使用） |
| K9-Pad 菜单 | `WouoUI_k9pad.c` | K9-Pad 专用菜单定义 |
| 平台接口 | `WouoUI_port.c/h` | Rust FFI 入口 |
| 配置 | `WouoUI_conf.h` | 屏幕尺寸、动画参数 |
| 公共头 | `WouoUI_common.h` | 类型、宏、内存函数声明 |
| 类型定义 | `wououi_types.h` | bare-metal stdint 替代 |
