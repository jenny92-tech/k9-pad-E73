# K9-Pad 全链路逻辑线稿 v1.0

> 基于当前代码逐行核对的真实状态。所有线稿 = 状态机 + 数据流 + 业务流。

---

## 1. 整体架构

```
┌─────────────────────────── Host (Mac 应用) ───────────────────────────┐
│  GPUI 主窗口(NotchPanel) ←── 刘海面板状态机 ←── AppState                │
│      │                                                                 │
│      │ AppEvent / SlotUpdate / PermissionRequest                        │
│      ▼                                                                 │
│  bridge_loop (GPUI 侧) ── 事件上行 + 命令下行 (std mpsc) ── tokio 线程  │
│                                                                        │
│  ┌─ tokio_main ─────────────────────────────────────────────────┐     │
│  │  BLE connect(重试循环) → K9Client                              │     │
│  │  get_caps → Connected → 发布局+组件 → dispatcher 循环          │     │
│  │  dispatcher: provider 更新 → comp_set*（wake=false）           │     │
│  │  dialog_rx → show_dialog + dialog_option×N                     │     │
│  └───────────────────────────────────────────────────────────────┘     │
│  hook_server (Unix socket) ←── k9-hook-bridge ←── AI CLI (claude等)    │
└──────────────────────┬─────────────────────────────────────────────────┘
                       │ BLE GATT（数据通道服务 e9dc0001）
                       ▼
┌────────────────────── 固件 (nRF52840) ────────────────────────────────┐
│  run_data_channel: DATA_CHANNEL_RX → parse → DISPLAY_DATA/DIALOG_DATA  │
│                    DIALOG_RESULT/CONFIG → DATA_CHANNEL_TX → notify     │
│                                                                        │
│  run_display (主循环，下面详述):                                        │
│    MENU_INPUT 处理 / DIALOG_DATA / DISPLAY_DATA / 渲染                 │
│                                                                        │
│  menu_controller: 编码器/按键 → MENU_INPUT（menu_active||dialog 时）    │
│  RMK: MENU_MODE_ACTIVE 控制 keymap 吞键                                 │
└────────────────────────────────────────────────────────────────────────┘
```

---

## 2. 协议层（shared-datachannel-proto）

```
包格式: [CMD(1) | TYPE(1) | LEN(2) | payload]  ← 64B 上限

展示通道:
  LAYOUT_CFG(0x22)  rows,cols,show_status
  COMP_LAYOUT(0x23) id,row,col,kind,label_len,label
  COMP_SET(0x24)    id,flags(wake),value(TYPE 区分: Text/Numeric/Progress/Percentage/Checkbox/Icon)

对话通道:
  ShowDialog(0x20)  id,kind(ConfirmCancel|Choice),title
  DialogOption(0x25) id,index,label
  DialogResult(0x21) id,selection,code(Confirm/Cancel/Timeout)  ← 设备→主机

控制:
  GetStatus/StatusResp/Ping/Pong/GetCapabilities/CapabilitiesResp/ConfigChanged
```

---

## 3. Host 侧状态机

### 3.1 连接生命周期（tokio_main）

```
start → Connecting
  → loop{ BleTransport::connect(10s) }  ← 失败 sleep 5s 重试
  → Connected
  → get_capabilities / get_status（成功才进 dispatcher，失败仍进）
  → 发布局: set_layout(2,2,true) + comp_layout×4（Time/Vol/Subs/AI）
  → dispatcher 循环:
      provider_rx.recv → comp_set*(comp_id=slot+1, wake=false)
        slot3(AI) → comp_set_percentage
      dialog_rx.recv → show_dialog(kind,title) + dialog_option×N
  → 传输断 → Error → 整个 tokio_main 结束（无重连！）
```

### 3.2 刘海面板状态机（AppState 驱动）

```
Hidden ──(有连接/slot/会话)──→ Widened ──(permission_queue 非空)──→ Dropped
  ↑                              │                                        │
  └────────(清空)────────────────┘←──────(队列空)────────────────────────┘
动画: 展开=先横扩再下拉（重叠 1.5px 阈值）；收回=先纵缩再横缩
窗口: 激活期固定 (max_w, 状态高)；形状在窗口内动画（避免逐帧 resize）
```

### 3.3 审批业务（hook_server → 设备弹窗 → 结果回传）

```
AI CLI → k9-hook-bridge → hook_server(Unix socket)
  → PermissionRequest → ① 入 approval 队列(UI 渠道)
                       ② dialog_map[dialog_id] = req_id
                       ③ DialogRequest{kind:ConfirmCancel, title, options:[Allow,Deny]}
                        → dispatcher → show_dialog + dialog_option×2
设备选择 → DialogResult{id, selection, code} → bridge
  → dialog_map.remove(id) → req_id
  → dialog_decision(selection, code): Confirm&&sel0=allow, 其余=deny
  → resolve_pending(req_id, decision) → oneshot 完成 → hook 响应 → AI CLI
```

---

## 4. 固件显示主循环（run_display 每帧）

```
loop {
  ┌─ 输入处理: while MENU_INPUT.try_receive() {
  │    screen_off → 唤醒 + (quick_menu?) → continue
  │    dialog_active → ScrollUp/Down 切选项; Select→Confirm; Back→Cancel
  │    menu_active → 转发 wououi (EnterMenu/Back/Scroll/Select)
  │    home → EnterMenu 进菜单; Back 退菜单
  │  }
  │
  ├─ DIALOG_DATA drain: ShowDialog → 强制亮屏 + dialog_* 状态重置 + MENU_MODE_ACTIVE=true
  │
  ├─ DISPLAY_DATA drain（主循环，任何模式）:
  │    comp_cache.apply / dc_cache.apply
  │    DialogOption → dialog_options[id][index]（dialog_active && id 匹配时）
  │    SetCompValue wake=true → 唤醒屏幕
  │
  ├─ battery / BLE 事件
  │
  └─ 渲染 if screen_on:
       dialog_active → draw_dialog（标题+选项+反白高亮）
       else menu_active → wououi.tick + copy buffer
       else → 首页: has_comps ? draw_component_grid : draw_keyboard_ui
             + 睡眠检查（!dialog_active 时）
}
```

### 4.1 组件网格渲染

```
CompCache { rows, cols, show_status, comps[8] }  ← comps 按 id 索引(1-4 用)
draw_component_grid:
  状态栏(show_status): 模式名 + BLE + 电池 (13px)
  网格: cell_w = 128/cols, cell_h = (64-状态栏)/rows
  每格: draw_comp — 第一行 label+值(6x10)，Progress 再加进度条
```

### 4.2 弹窗选择器

```
dialog_active=true (由 ShowDialog 触发，锁屏)
状态: dialog_id / dialog_kind / dialog_title / dialog_options[4] / dialog_selection
输入: 滚轮± → selection; Select → Result{sel,Confirm}; Back → Result{0,Cancel}
渲染: 标题 + 分隔线 + 选项(选中反白)
关闭: 确认/取消/超时(菜单空闲超时复用) → DIALOG_RESULT → 恢复 MENU_MODE_ACTIVE
```

---

## 5. 固件输入路由

```
编码器(RotaryEncoder) ──┐
W4B152110(Select) ──────┼──→ menu_controller ──→ MENU_INPUT ──→ display
SW1(ROW0/COL3, deferred)─┘     ↑ 仅 menu_active || dialog_active 时转发

keymap 吞键: RMK MENU_MODE_ACTIVE=true 时:
  编码器(ENCODER_INTERCEPT) + Select(menu_intercept) → 不发给主机(不调音量)
  menu_controller 同时观察事件 → 发 MenuInput

弹窗时: display 设置 MENU_MODE_ACTIVE=true（吞键）+ DIALOG_ACTIVE=true（controller 转发）
```

---

## 6. 数据通道（BLE）

```
Host write → GATT rx_from_host → gatt_events_task:
  data.len() 1..=64 → DATA_CHANNEL_RX.try_send([u8;64])
run_data_channel:
  DATA_CHANNEL_RX.receive → parse_display/component/dialog/control
    → DISPLAY_DATA / DIALOG_DATA / DATA_CHANNEL_TX
run_ble_data_channel: DATA_CHANNEL_TX → tx_to_host.notify → Host 读流

并发连接: connection_loop select4（advertise + 槽A + 槽B + profile）
  2 条连接并行服务（HID + app 数据通道）
```

---

## 7. 已知待确认点（线稿 vs 代码）

- [ ] dispatcher 断连后 tokio_main 结束，主桥**不自动重连**（初连重试循环只包初次）
- [ ] 弹窗出现时 wououi 菜单状态（menu_active）是否被正确保留/恢复
- [ ] 弹窗叠加在菜单上时，wououi.tick 冻结是否影响 C 侧状态
- [ ] AI 多选弹窗的 host 侧结果 → AI agent 链路（hook 协议选项扩展未做）
