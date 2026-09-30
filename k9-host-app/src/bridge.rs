// INPUT:  k9-host-lib (BleTransport, K9Client, DeviceEvent), providers, app_state, session, hook_server, tokio, std::sync::mpsc
// OUTPUT: start_tokio_thread(), bridge_loop(), HostCommand, DialogRequest — tokio-GPUI 跨运行时桥接（事件上行 + 命令下行）
// POS:    运行时桥接层 — 在独立 OS 线程启动 tokio runtime，通过 std::sync::mpsc 与 GPUI 双向传递状态事件和命令；hook socket server 也在此 runtime spawn；
//         设备应答渠道：DialogResult 命中 dialog→req 映射时完成审批或单选问题 oneshot，并投 AppEvent::PermissionAnswered；
//         Phase 4：看门狗 60s tick 投 AppEvent::WatchdogSweep → GPUI sweep 会话（waiting 300s 强制 idle / idle 600s 移除），
//         受影响 pending 经 HostCommand::ResolvePermissions 回流 tokio deny 完成 oneshot

use std::sync::mpsc;
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::{Duration, SystemTime};

use gpui::AsyncApp;
use log::{error, info, warn};

use k9_datachannel_proto::CompKind;
use k9_host_lib::{BleTransport, DeviceEvent, K9Client, Transport};

use crate::app_state::{
    self, AppEvent, AppState, ConnectionStatus, PendingPermission, SlotContent,
};
use crate::hook_server;
use crate::providers::ai_quota::AiQuotaProvider;
use crate::providers::bilibili::BilibiliProvider;
use crate::providers::time::TimeProvider;
use crate::providers::volume::VolumeProvider;
use crate::providers::{DisplayData, DisplayUpdate, Provider};
use crate::session::{self, AgentStatus, HookEvent};

/// Commands sent from the GPUI UI to the tokio bridge thread.
pub enum HostCommand {
    /// Show a confirm/cancel dialog on the device (fire-and-forget).
    ShowDialog { text: String },
    /// 审批决策（Allow/Deny）——完成 pending map 里对应的 oneshot，先答先生效
    AnswerPermission { req_id: u64, decision: String },
    /// AskUserQuestion 的完整答案——写回 PermissionRequest.updatedInput.answers
    AnswerQuestions {
        req_id: u64,
        answers: serde_json::Map<String, serde_json::Value>,
    },
    /// 批量完成审批 oneshot（watchdog sweep 等 GPUI 侧 drain 回流；先答先生效，无 waiter 忽略）
    ResolvePermissions { req_ids: Vec<u64>, decision: String },
}

/// 看门狗 sweep 间隔（waiting 300s / idle 600s 阈值的最小粒度，蓝本 cleanup timer 同量级）
const WATCHDOG_INTERVAL: Duration = Duration::from_secs(60);

/// 设备弹窗下发请求（dialog dispatcher 队列元素）：
/// `id` 为 None 时由 dispatcher 自增分配；审批渠道携带 req_id 派生的固定 id
pub struct DialogRequest {
    pub id: Option<u8>,
    /// 弹窗类型（授权确认 / AI 多选）
    pub kind: k9_datachannel_proto::DialogKind,
    /// 标题（单行）
    pub title: String,
    /// 选项标签（≤4 个；ConfirmCancel 一般 2 个：Allow/Deny）
    pub options: Vec<String>,
}

/// Start the tokio runtime on a dedicated OS thread.
///
/// Returns the event receiver (for the GPUI bridge loop), the command sender
/// (for the UI), and the thread handle.
pub fn start_tokio_thread() -> (
    mpsc::Receiver<AppEvent>,
    mpsc::Sender<HostCommand>,
    JoinHandle<()>,
) {
    let (event_tx, event_rx) = mpsc::channel();
    let (cmd_tx, cmd_rx) = mpsc::channel();

    let handle = std::thread::spawn(move || {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("Failed to create tokio runtime");

        rt.block_on(tokio_main(event_tx, cmd_rx));
    });

    (event_rx, cmd_tx, handle)
}

/// Main logic running inside the tokio runtime.
async fn tokio_main(event_tx: mpsc::Sender<AppEvent>, cmd_rx: mpsc::Receiver<HostCommand>) {
    let _ = event_tx.send(AppEvent::ConnectionChanged(ConnectionStatus::Connecting));

    // 命令处理独立于 BLE：AnswerPermission 不等设备连接（审批无设备也要能答），
    // ShowDialog / 审批弹窗转发到设备连接后的 dispatcher
    let (dialog_tx, mut dialog_rx) = tokio::sync::mpsc::unbounded_channel::<DialogRequest>();

    // Hook socket server 独立于 BLE：设备未连接时也要能收 hook 事件
    let (pending_permissions, dialog_map) =
        hook_server::spawn_hook_server(event_tx.clone(), dialog_tx.clone());
    tokio::spawn(host_command_loop(
        cmd_rx,
        pending_permissions.clone(),
        dialog_map.clone(),
        dialog_tx,
    ));

    // 看门狗 tick：60s 一次投 WatchdogSweep，GPUI 侧 sweep_sessions 后
    // 经 ResolvePermissions 回流 deny 受影响 pending（会话状态归 GPUI 所有，故 tick 驱动）
    {
        let event_tx = event_tx.clone();
        tokio::spawn(async move {
            let mut tick = tokio::time::interval(WATCHDOG_INTERVAL);
            loop {
                tick.tick().await;
                if event_tx.send(AppEvent::WatchdogSweep).is_err() {
                    return;
                }
            }
        });
    }

    // 连接 + 会话循环：断连后自动重连（不只初次）——
    // 刘海/设备解耦后，主桥必须持续保持连接，否则内容推送全断。
    'conn: loop {
        // BLE connection retry loop
        let transport = loop {
            info!("Scanning for K9-Pad...");
            match BleTransport::connect(Duration::from_secs(10)).await {
                Ok(t) => break t,
                Err(e) => {
                    let msg = format!("BLE connect failed: {e}");
                    warn!("{msg}");
                    let _ =
                        event_tx.send(AppEvent::ConnectionChanged(ConnectionStatus::Error(msg)));
                    tokio::time::sleep(Duration::from_secs(5)).await;
                }
            }
        };

        let client = Arc::new(K9Client::new(transport));

        // Fetch device capabilities（开发阶段 V1：弹窗不再做协议版本门槛）
        match client.get_capabilities().await {
            Ok(caps) => {
                info!(
                    "Device: FW {}.{}.{} | Protocol v{}",
                    caps.firmware_major,
                    caps.firmware_minor,
                    caps.firmware_patch,
                    caps.protocol_version
                );
                let _ = event_tx.send(AppEvent::DeviceCaps(caps));
            }
            Err(e) => warn!("Failed to get capabilities: {e}"),
        }

        // Fetch pad config (UI display only; not used for provider decisions)
        match client.get_status().await {
            Ok(config) => {
                info!("Pad config: active_pad={}", config.active_pad);
                let _ = event_tx.send(AppEvent::PadConfigUpdated(config));
            }
            Err(e) => warn!("Failed to get status: {e}"),
        }

        let _ = event_tx.send(AppEvent::ConnectionChanged(ConnectionStatus::Connected));

        // Forward device-initiated events (dialog results, config changes) to the UI.
        {
            let mut device_event_rx = client.subscribe_events();
            let event_tx = event_tx.clone();
            // 每次连接克隆：重连后旧任务持有的 Arc 已消费，不能复用
            let dialog_map = dialog_map.clone();
            let pending_permissions = pending_permissions.clone();
            tokio::spawn(async move {
                loop {
                    match device_event_rx.recv().await {
                        Ok(DeviceEvent::DialogResult {
                            id,
                            selection,
                            result,
                        }) => {
                            // 审批弹窗结果：命中 dialog→req 映射则走审批应答（先答先生效，
                            // UI 已答则无 waiter 忽略），并投 PermissionAnswered 让 GPUI 出队；
                            // 未命中（普通测试弹窗）走原 DialogResult 路径。
                            // selection：ConfirmCancel 弹窗 0=Allow 1=Deny；Cancel/Timeout → deny。
                            let req_id = dialog_map.lock().unwrap().remove(&id);
                            match req_id {
                                Some(req_id) => {
                                    if let Some(answer) = hook_server::resolve_dialog_selection(
                                        &pending_permissions,
                                        req_id,
                                        selection,
                                        result,
                                    ) {
                                        info!(
                                            "Request #{req_id} answered on device (dialog #{id}): {answer}"
                                        );
                                    } else {
                                        info!(
                                            "Request #{req_id} already answered, device result ignored"
                                        );
                                    }
                                    let _ = event_tx.send(AppEvent::PermissionAnswered { req_id });
                                }
                                None => {
                                    let _ = event_tx.send(AppEvent::DialogResult { id, result });
                                }
                            }
                        }
                        Ok(DeviceEvent::ConfigChanged(config)) => {
                            let _ = event_tx.send(AppEvent::PadConfigUpdated(config));
                        }
                        Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                            warn!("Missed {n} device events (lagged)");
                        }
                        Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                    }
                }
            });
        }

        // Provider dispatch channel
        let (provider_tx, mut provider_rx) = tokio::sync::mpsc::channel::<DisplayUpdate>(64);

        spawn_providers(provider_tx);

        // Dispatcher loop: forward provider updates to the keyboard, and drain
        // ShowDialog requests queued by the (BLE-independent) host command loop.
        //
        // 组件布局（新架构）：host 声明一次网格 + 组件，之后只推组件值。
        // slot 0..3 → 组件 1..4（slot+1）。
        let _ = client.set_layout(2, 2, true).await;
        let _ = client.comp_layout(1, 0, 0, CompKind::Text, "Time").await;
        let _ = client.comp_layout(2, 0, 1, CompKind::Progress, "Vol").await;
        let _ = client.comp_layout(3, 1, 0, CompKind::Numeric, "Subs").await;
        let _ = client
            .comp_layout(4, 1, 1, CompKind::Percentage, "AI")
            .await;
        loop {
            tokio::select! {
                update = provider_rx.recv() => {
                    let Some(update) = update else { break };
                    // 组件 ID = slot + 1；Progress/Percentage 同用 u8 编码
                    let comp_id = update.slot + 1;
                    // wake=false：时间/音量等常规更新不唤醒屏幕（时钟跳动不该把屏弄醒）。
                    // slot 3 = AI 配额 = Percentage 组件（值编码同为 u8，但 DataType 要区分）
                    let result = match &update.data {
                        DisplayData::Text(text) => client.comp_set_text(comp_id, false, text).await,
                        DisplayData::Numeric(value) => client.comp_set_numeric(comp_id, false, *value).await,
                        DisplayData::Progress(value) => {
                            if update.slot == 3 {
                                client.comp_set_percentage(comp_id, false, *value).await
                            } else {
                                client.comp_set_u8(comp_id, false, *value).await
                            }
                        }
                    };

                    match result {
                        Ok(()) => {
                            // 推送成功后把 slot 数据镜像给 UI（刘海浮窗数据源）
                            let content = match &update.data {
                                DisplayData::Text(text) => SlotContent::Text(text.clone()),
                                DisplayData::Numeric(value) => SlotContent::Numeric(*value),
                                DisplayData::Progress(value) => SlotContent::Progress(*value),
                            };
                            let _ = event_tx.send(AppEvent::SlotUpdate {
                                slot: update.slot,
                                content: Some(content),
                            });
                        }
                        Err(e) => {
                            error!("Push failed: {e}");
                            if !client.transport().is_connected() {
                                let _ = event_tx.send(AppEvent::ConnectionChanged(
                                    ConnectionStatus::Error("Device disconnected".into()),
                                ));
                                break;
                            }
                        }
                    }
                }
                req = dialog_rx.recv() => {
                    let Some(req) = req else { break };
                    // 开发阶段 V1 直接发：不做协议版本门槛
                    // id:None 的测试弹窗固定用 0——req_to_dialog_id 值域 1..=255，0 安全不冲突
                    let id = req.id.unwrap_or(0);
                    // 标题 + 选项逐个下发（设备攒齐后全屏显示，滚轮/按键选择）
                    let _ = client.show_dialog(id, req.kind, &req.title).await;
                    for (i, opt) in req.options.iter().enumerate() {
                        let _ = client.dialog_option(id, i as u8, opt).await;
                    }
                    info!("Sent dialog #{id} ({} options): {}", req.options.len(), req.title);
                }
            }
        } // end dispatcher loop

        // 会话结束（断连）：dialog 通道还开着就重连，关了说明 hook 服务没了 → 退出
        if dialog_rx.is_closed() {
            break 'conn;
        }
        info!("Device disconnected — reconnecting...");
    }
}

/// UI 命令循环：100ms tick 轮询 std-mpsc 命令通道（BLE 未连接时也在跑）。
///
/// - AnswerPermission：立即完成 pending map 里的 oneshot（先答先生效，无 waiter 忽略）
/// - ShowDialog：转发给设备连接后的 dispatcher 循环（id 自增分配）
async fn host_command_loop(
    cmd_rx: mpsc::Receiver<HostCommand>,
    pending: hook_server::PendingMap,
    dialogs: hook_server::DialogMap,
    dialog_tx: tokio::sync::mpsc::UnboundedSender<DialogRequest>,
) {
    let mut tick = tokio::time::interval(Duration::from_millis(100));
    loop {
        tick.tick().await;
        loop {
            match cmd_rx.try_recv() {
                Ok(HostCommand::ShowDialog { text }) => {
                    // 测试弹窗：默认确认/取消（ConfirmCancel，Allow/Deny）
                    if dialog_tx
                        .send(DialogRequest {
                            id: None,
                            kind: k9_datachannel_proto::DialogKind::ConfirmCancel,
                            title: text,
                            options: vec!["Allow".to_string(), "Deny".to_string()],
                        })
                        .is_err()
                    {
                        return;
                    }
                }
                Ok(HostCommand::AnswerPermission { req_id, decision }) => {
                    // 先答先生效：remove 即消费，无 waiter（已被另一渠道应答）则忽略；
                    // 非设备渠道解决时同步清 dialog_map，防 id 回绕后误答
                    if !hook_server::resolve_pending(&pending, req_id, &decision) {
                        info!("Permission #{req_id} already answered, ignored");
                    }
                    dialogs
                        .lock()
                        .unwrap()
                        .remove(&hook_server::req_to_dialog_id(req_id));
                }
                Ok(HostCommand::AnswerQuestions { req_id, answers }) => {
                    if !hook_server::resolve_questions(&pending, req_id, answers) {
                        info!("Question #{req_id} already answered, ignored");
                    }
                    dialogs
                        .lock()
                        .unwrap()
                        .remove(&hook_server::req_to_dialog_id(req_id));
                }
                Ok(HostCommand::ResolvePermissions { req_ids, decision }) => {
                    // GPUI 侧 drain（watchdog sweep 等）回流：逐个完成 oneshot，无 waiter 跳过；
                    // 同步清 dialog_map（防残留条目在 id 回绕后误答）
                    for req_id in req_ids {
                        hook_server::resolve_pending(&pending, req_id, &decision);
                        dialogs
                            .lock()
                            .unwrap()
                            .remove(&hook_server::req_to_dialog_id(req_id));
                    }
                }
                Err(mpsc::TryRecvError::Empty) => break,
                Err(mpsc::TryRecvError::Disconnected) => return,
            }
        }
    }
}

/// Spawn all provider tasks unconditionally.
///
/// Display responsibility is push-driven: the device shows whatever slots have
/// data, so the host always runs every provider regardless of device-side config.
fn spawn_providers(tx: tokio::sync::mpsc::Sender<DisplayUpdate>) {
    {
        let tx = tx.clone();
        tokio::spawn(async move {
            let mut p = TimeProvider::new(0, "%H:%M".into());
            if let Err(e) = p.start(tx).await {
                warn!("Time provider exited: {e}");
            }
        });
    }

    {
        let tx = tx.clone();
        tokio::spawn(async move {
            let mut p = VolumeProvider::new(1);
            if let Err(e) = p.start(tx).await {
                warn!("Volume provider exited: {e}");
            }
        });
    }

    {
        let tx = tx.clone();
        tokio::spawn(async move {
            // TODO: make uid configurable
            let mut p = BilibiliProvider::new(2, 0, 300);
            if let Err(e) = p.start(tx).await {
                warn!("Bilibili provider exited: {e}");
            }
        });
    }

    tokio::spawn(async move {
        let mut p = AiQuotaProvider::new(3);
        if let Err(e) = p.start(tx).await {
            warn!("AI quota provider exited: {e}");
        }
    });
}

/// 把 hook 事件灌进 session reducer（SessionEnd 时移除会话）
fn apply_hook_event(state: &mut AppState, ev: &HookEvent) {
    let id = ev.session_id.clone();
    let sess = state
        .sessions
        .entry(id.clone())
        .or_insert_with(|| session::SessionState::new(id.clone(), ev.source.clone()));
    if matches!(session::reduce(sess, ev), session::ReduceOutcome::Remove) {
        state.sessions.remove(&id);
    }
}

/// GPUI-side bridge loop: drains events from the tokio thread and updates AppState.
///
/// Runs as a GPUI foreground async task, polling the std::sync::mpsc receiver
/// at 50ms intervals to avoid blocking the UI thread.
pub async fn bridge_loop(rx: mpsc::Receiver<AppEvent>, cx: &mut AsyncApp) {
    loop {
        cx.background_executor()
            .timer(Duration::from_millis(50))
            .await;

        // Drain all pending events
        loop {
            match rx.try_recv() {
                Ok(event) => {
                    let should_exit = matches!(event, AppEvent::Shutdown);

                    let _ = cx.update_global::<AppState, _>(|state, _cx| match event {
                        AppEvent::ConnectionChanged(status) => state.connection = status,
                        AppEvent::DeviceCaps(caps) => {
                            state.dialog_supported = true;
                            state.device_caps = Some(caps);
                        }
                        AppEvent::PadConfigUpdated(config) => state.pad_config = Some(config),
                        AppEvent::DialogResult { id, result } => {
                            state.last_dialog_result = Some((id, result));
                            state.dialog_result_count += 1;
                        }
                        AppEvent::SlotUpdate { slot, content } => {
                            if (slot as usize) < state.slots.len() {
                                state.slots[slot as usize] = content;
                            }
                        }
                        AppEvent::HookEvent(ev) => {
                            apply_hook_event(state, &ev);
                            // Phase 4 队列镜像同步（决策已由 tokio 侧完成 oneshot 给出）：
                            // - SessionEnd：会话已被 reducer 移除，其 pending 全部出队（tokio 已 deny）；
                            // - 活动/完成事件：「已在终端批准」orphan + 同 tool_use_id 条目出队；
                            //   出队后会话仍 waiting 且名下已无 pending → 回 Processing（蓝本 wasWaiting 块语义；
                            //   Stop 是权威事件已被 reducer 置 Idle，不在此列）
                            if ev.event_name == "SessionEnd" {
                                app_state::remove_session_from_queue(
                                    &mut state.permission_queue,
                                    &ev.session_id,
                                );
                            } else {
                                app_state::drain_queue_on_activity(
                                    &mut state.permission_queue,
                                    &ev,
                                );
                                let stuck_waiting = state
                                    .sessions
                                    .get(&ev.session_id)
                                    .map(|s| s.status.is_waiting())
                                    .unwrap_or(false)
                                    && !state
                                        .permission_queue
                                        .iter()
                                        .any(|p| p.session_id == ev.session_id);
                                if stuck_waiting {
                                    if let Some(sess) = state.sessions.get_mut(&ev.session_id) {
                                        sess.status = AgentStatus::Processing;
                                        sess.current_tool = None;
                                    }
                                }
                            }
                        }
                        AppEvent::PermissionRequest {
                            req_id,
                            session_id,
                            source,
                            tool_name,
                            summary,
                            prompt,
                            tool_use_id,
                        } => {
                            // reducer 按 prompt 置 WaitingApproval/WaitingQuestion，然后入全局 FIFO 请求队列
                            let ev = HookEvent {
                                event_name: "PermissionRequest".into(),
                                session_id: session_id.clone(),
                                source: source.clone(),
                                tool_name: if tool_name.is_empty() {
                                    None
                                } else {
                                    Some(tool_name.clone())
                                },
                                message: None,
                                has_question: false,
                                tool_use_id: tool_use_id.clone(),
                            };
                            apply_hook_event(state, &ev);
                            let req = PendingPermission {
                                req_id,
                                session_id,
                                source,
                                tool_name,
                                summary,
                                prompt,
                                tool_use_id,
                            };
                            // 同 tool_use_id 重复请求：原位替换保持卡片位置
                            // （tokio 侧已 deny 旧 waiter），无重复则照常入队尾
                            if !app_state::replace_duplicate_in_queue(
                                &mut state.permission_queue,
                                req.clone(),
                            ) {
                                state.permission_queue.push_back(req);
                            }
                        }
                        AppEvent::Shutdown => state.connection = ConnectionStatus::Disconnected,
                        AppEvent::WatchdogSweep => {
                            // waiting 300s 强制 Idle / idle 600s 移除（session::sweep_sessions），
                            // 受影响会话的 pending 出队并回流 tokio deny 完成 oneshot
                            let actions =
                                session::sweep_sessions(&mut state.sessions, SystemTime::now());
                            if !actions.is_empty() {
                                let mut deny_ids = Vec::new();
                                for (sid, _) in &actions {
                                    deny_ids.extend(app_state::remove_session_from_queue(
                                        &mut state.permission_queue,
                                        sid,
                                    ));
                                }
                                info!(
                                    "watchdog sweep: {} session(s) affected, {} pending denied",
                                    actions.len(),
                                    deny_ids.len()
                                );
                                if !deny_ids.is_empty() {
                                    if let Some(tx) = &state.host_command_tx {
                                        let _ = tx.send(HostCommand::ResolvePermissions {
                                            req_ids: deny_ids,
                                            decision: "deny".into(),
                                        });
                                    }
                                }
                            }
                        }
                        AppEvent::PermissionAnswered { req_id } => {
                            // 设备渠道已应答：出队（幂等，UI 已答则队列里没有，跳过）
                            // + 会话状态回 Processing；面板由 AppState observe 自动收回
                            if let Some(pos) = state
                                .permission_queue
                                .iter()
                                .position(|p| p.req_id == req_id)
                            {
                                if let Some(p) = state.permission_queue.remove(pos) {
                                    if let Some(sess) = state.sessions.get_mut(&p.session_id) {
                                        if sess.status.is_waiting() {
                                            sess.status = AgentStatus::Processing;
                                        }
                                    }
                                }
                            }
                        }
                    });

                    if should_exit {
                        return;
                    }
                }
                Err(mpsc::TryRecvError::Empty) => break,
                Err(mpsc::TryRecvError::Disconnected) => return,
            }
        }

        // 审批队列/会话/连接变化已由 update_global 触发 AppState observe，
        // 刘海面板（notch_panel）自行驱动状态机，无需在此同步窗口
    }
}
