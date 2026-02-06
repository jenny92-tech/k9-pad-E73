# K9-Pad 菜单系统文档

> **重要**: 修改代码后必须同步更新本文档！

---

## 1. 控制模式

### 1.1 硬件输入设备

| 设备 | 物理位置 | 矩阵位置 | GPIO |
|------|---------|---------|------|
| **SW1** | 键盘左上角 | ROW0/COL3 | P1.11, P0.31 |
| **TTC 编码器** | 键盘顶部 | - | A相=P0.10, B相=P0.09 |
| **W4B152110** | 编码器下方 | ROW0/COL2 | P1.11, P0.29 |

### 1.2 按键功能定义

#### 正常模式（菜单未激活）

| 输入 | 动作 | 结果 |
|------|------|------|
| SW1 短按 (< 300ms) | 发送按键 | 发送 ESC 键码 |
| SW1 长按 (≥ 500ms) | 进入菜单 | 菜单激活，显示主菜单 |
| 编码器顺时针 | 发送按键 | 发送配置的键码 |
| 编码器逆时针 | 发送按键 | 发送配置的键码 |
| W4B152110 | 发送按键 | 发送配置的键码 |

#### 菜单模式（菜单已激活）

| 输入 | 动作 | 结果 |
|------|------|------|
| SW1 短按 | 返回/退出 | 主菜单→退出菜单；子菜单→返回上级 |
| SW1 长按 | 无效 | 忽略 |
| 编码器顺时针 | 向下滚动 | selected_index + 1 |
| 编码器逆时针 | 向上滚动 | selected_index - 1 |
| W4B152110 | 确认选择 | 进入子菜单或执行操作 |

### 1.3 时间参数

| 参数 | 值 | 说明 |
|------|-----|------|
| 长按阈值 | 500ms | SW1 按住超过此时间触发长按 |
| 短按最大时长 | 300ms | SW1 释放时间小于此值为短按 |
| 自动退出超时 | 30秒 | 菜单无操作自动返回首页 |
| 编码器去抖 | 5ms | 防止编码器抖动 |

---

## 2. 菜单层级结构

### 2.1 层级概览

```
菜单层级: 2 级
├── 第 1 级: 主菜单 (MainMenu)
└── 第 2 级: 子菜单 (ModeSelect, BleSettings, About)
```

### 2.2 导航栈

- 最大深度: **4 级**（代码中定义）
- 当前使用: **2 级**

### 2.3 页面导航图

```
┌─────────────────────────────────────────────────────────┐
│                      首页 (Home)                         │
│                   active = false                        │
│                                                         │
│                      [MEDIA]                            │
│                                        🔋 📶            │
└─────────────────────────────────────────────────────────┘
                          │
                          │ SW1 长按 (500ms)
                          ▼
┌─────────────────────────────────────────────────────────┐
│                   主菜单 (MainMenu)                      │
│                   active = true                         │
│                   nav_stack = []                        │
│  ┌───────────────────────────────────────────────────┐  │
│  │  Menu                                             │  │
│  │  ─────────────────────────────────────────────    │  │
│  │  > Mode                    ← 导航到 ModeSelect    │  │
│  │    Bluetooth               ← 导航到 BleSettings   │  │
│  │    About                   ← 导航到 About         │  │
│  └───────────────────────────────────────────────────┘  │
│                                                         │
│  SW1 短按 → 退出菜单，回到首页                            │
└─────────────────────────────────────────────────────────┘
          │                    │                    │
          │ W4B152110          │ W4B152110          │ W4B152110
          │ (选中 Mode)        │ (选中 Bluetooth)   │ (选中 About)
          ▼                    ▼                    ▼
┌─────────────────┐  ┌─────────────────┐  ┌─────────────────┐
│   模式选择       │  │   蓝牙设置       │  │     关于        │
│  (ModeSelect)   │  │ (BleSettings)   │  │    (About)      │
│                 │  │                 │  │                 │
│  nav_stack =    │  │  nav_stack =    │  │  nav_stack =    │
│  [MainMenu]     │  │  [MainMenu]     │  │  [MainMenu]     │
│  ───────────    │  │  ───────────    │  │  ───────────    │
│  Mode           │  │  Bluetooth      │  │  About          │
│  ───────────    │  │  ───────────    │  │  ───────────    │
│  > MEDIA    *   │  │  > Disconnect   │  │  K9-Pad E73     │
│    EXCEL        │  │    Clear Pairing│  │  FW: v0.2.0     │
│    CLAUDE       │  │                 │  │  RMK Based      │
│                 │  │                 │  │  (不可选)        │
│  * = 当前模式    │  │                 │  │                 │
└─────────────────┘  └─────────────────┘  └─────────────────┘
        │                    │                    │
        │ SW1 短按           │ SW1 短按           │ SW1 短按
        └────────────────────┴────────────────────┘
                             │
                             ▼
                    返回主菜单 (MainMenu)
```

---

## 3. 页面结构树

```
页面总数: 5
菜单层级: 2

Root
│
├── [Home] 首页 (非菜单页面)
│   │
│   │ SW1 长按 500ms
│   ▼
│
└── [MainMenu] 主菜单
    │   标题: "Menu"
    │   返回: 无 (SW1 短按退出)
    │
    ├── [0] "Mode" ──────────────► [ModeSelect] 模式选择
    │                               │   标题: "Mode"
    │                               │   返回: SW1 短按
    │                               │
    │                               ├── [0] "MEDIA"   → KeyboardMode::Media  (当前模式显示 *)
    │                               ├── [1] "EXCEL"   → KeyboardMode::Excel
    │                               └── [2] "CLAUDE"  → KeyboardMode::Claude
    │
    ├── [1] "Bluetooth" ─────────► [BleSettings] 蓝牙设置
    │                               │   标题: "Bluetooth"
    │                               │   返回: SW1 短按
    │                               │
    │                               ├── [0] "Disconnect"     (功能待实现)
    │                               └── [1] "Clear Pairing"  (功能待实现)
    │
    └── [2] "About" ─────────────► [About] 关于
                                    │   标题: "About"
                                    │   返回: SW1 短按
                                    │
                                    ├── [0] "K9-Pad E73"  (不可选, 纯显示)
                                    ├── [1] "FW: v0.2.0"  (不可选, 纯显示)
                                    └── [2] "RMK Based"   (不可选, 纯显示)
```

### 3.1 页面统计

| 统计项 | 数量 |
|--------|------|
| 总页面数 | **5** |
| 菜单页面 | 4 (MainMenu, ModeSelect, BleSettings, About) |
| 非菜单页面 | 1 (Home) |
| 总菜单项 | 11 |

### 3.2 页面属性对照表

| PageId | 标题 | 项数 | 可选项 | 显示返回 | 特殊行为 |
|--------|------|------|--------|---------|---------|
| `Home` | - | 0 | - | - | 显示模式/电池/蓝牙状态 |
| `MainMenu` | "Menu" | 3 | 3 | 否 | 根菜单，SW1 短按退出 |
| `ModeSelect` | "Mode" | 3 | 3 | 是 | 当前模式显示 `*`，选择后切换 |
| `BleSettings` | "Bluetooth" | 2 | 2 | 是 | 功能待实现 |
| `About` | "About" | 3 | 0 | 是 | 纯信息展示，不可选择 |

### 3.3 修改示例

**添加菜单项**: 在 `page.rs` 对应的 `ITEMS` 数组中添加

```rust
// 例：在主菜单添加 "Settings"
pub static MAIN_MENU_ITEMS: &[MenuItem] = &[
    MenuItem::with_target("Mode", PageId::ModeSelect),
    MenuItem::with_target("Bluetooth", PageId::BleSettings),
    MenuItem::with_target("Settings", PageId::Settings),  // 新增
    MenuItem::with_target("About", PageId::About),
];
```

**添加新页面**:
1. 在 `state.rs` 的 `PageId` 枚举添加新值
2. 在 `page.rs` 添加 `XXX_ITEMS` 和 `XXX_PAGE`
3. 在 `get_page_content()` 添加匹配分支
4. 更新本文档的结构树

---

## 4. 键盘模式

| 模式 | 枚举值 | 显示名称 | 说明 |
|------|--------|---------|------|
| Media | `KeyboardMode::Media` | "MEDIA" | 媒体控制 |
| Excel | `KeyboardMode::Excel` | "EXCEL" | 表格快捷键 |
| Claude | `KeyboardMode::Claude` | "CLAUDE" | AI 助手 |

---

## 5. 代码文件对照

| 功能 | 文件 | 关键定义 |
|------|------|---------|
| 页面定义 | `src/menu/page.rs` | `PageId`, `MAIN_MENU_ITEMS`, `MODE_SELECT_ITEMS` 等 |
| 状态机 | `src/menu/state.rs` | `MenuState`, `MenuInput`, `MenuStateMachine` |
| 渲染 | `src/menu/renderer.rs` | `MenuRenderer::render()` |
| **RMK 控制器** | `src/menu/controller.rs` | `MenuController`, `on_key_event()`, `poll()` |
| 输入处理（旧） | `src/menu/processor.rs` | `MenuInputProcessor`（备用方案） |
| 按键位置 | `keyboard.toml` | `[matrix]` 配置 |

---

## 5.1 RMK 控制器集成

菜单系统通过 RMK 的 `#[controller]` 宏集成到键盘事件流中：

```rust
// src/menu/controller.rs
#[controller(subscribe = [KeyEvent], poll_interval = 50)]
pub struct MenuController {
    sw1_press_time: Option<Instant>,
    sw1_long_triggered: bool,
    menu_active: bool,
}
```

### 工作原理

1. **事件订阅**：控制器订阅 `KeyEvent`，接收所有按键/编码器事件
2. **轮询模式**：每 50ms 调用 `poll()` 检测 SW1 长按
3. **事件转换**：将物理输入转换为 `MenuInput` 发送到状态机

### 注册方式

在 `src/main.rs` 的 `#[rmk_keyboard]` 模块中注册：

```rust
#[rmk_keyboard]
mod keyboard {
    use crate::menu::MenuController;
    use rmk::controller::PollingController;

    #[register_controller(poll)]
    fn menu_controller() -> MenuController {
        MenuController::new()
    }
}
```

### 限制

> **重要**：RMK 控制器只能**监听**事件，不能**拦截**事件。
> 因此在菜单模式下，按键仍会发送原始键码到主机。
> 如需完全拦截，需要在更低层级处理（如 InputProcessor）。

---

## 6. 修改检查清单

修改菜单系统时，请确保以下内容保持同步：

- [ ] `page.rs` 中的页面定义
- [ ] `state.rs` 中的 `PageId` 枚举
- [ ] `controller.rs` 中的按键位置常量（SW1_ROW/COL, SELECT_ROW/COL）
- [ ] `keyboard.toml` 中的矩阵配置
- [ ] `main.rs` 中的控制器注册
- [ ] 本文档

---

## 7. 版本历史

| 版本 | 日期 | 修改内容 |
|------|------|---------|
| v1.0 | 2024-XX-XX | 初始版本，5 个页面，2 级菜单 |
| v1.1 | 2025-02-05 | 集成 RMK controller，使用 `#[controller]` 宏订阅 KeyEvent |
