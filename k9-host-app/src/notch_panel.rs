// INPUT:  gpui, app_state (AppState, SlotContent, ConnectionStatus, PendingPermission/PendingPrompt), session (SessionState, AgentStatus),
//         bridge (HostCommand), spring (SpringVal + 蓝本 open/close/pop 参数), notch_shape (轮廓纯函数), notch_native (GPUI→NotchKit 适配), notchkit-core/macos (交互策略/指针追踪)
// OUTPUT: NotchPanelView + open_notch_panel_window() — 单窗口刘海变形面板（内容状态 + hover/click 展开 + 审批强制展开）
// POS:    刘海面板 — 一个 PopUp 透明窗口承载全部形态：黑色连续形状由 GPUI canvas 按 notch_shape::outline
//         逐帧绘制。窗口激活期尺寸固定（最大宽 × 状态高），形状在窗口内由弹簧积分驱动
//         （每帧 setFrame → CAMetalLayer setDrawableSize 丢弃内容 → 整窗闪动，故窗口只在状态
//         切换时 resize 一次）；非 Dropped 态整窗点击穿透（菜单栏可点），Dropped 态接收点击。
//         视觉蓝本 CodeIsland：0→1 从物理刘海横向「长出来」（宽度 ≥ 物理刘海宽，黑贴黑无缝），
//         审批时向下「拽出」下拉卡——v1 同宽设计（用户拍板）：上条与下拉卡永远同宽，
//         任意时刻都是一个黑色连续形状，W（宽）/ D（下拉高）两参数顺序动画
//         （展开先横扩再下拉、收回先纵缩再横缩），侧边直线垂落，底角 k=0.62 连续曲率。
//         收回先纵缩再横缩回物理刘海，到位后缩成 1x1 透明帧（0 状态一个像素都不画；
//         不能 orderOut——隐藏窗口收不到 drawRect，状态变化唤不醒渲染循环）
//         公共 InteractionController 负责 hover 延迟、点击固定与外部点击收回；PointerTracker
//         在窗口点击穿透时仍从 AppKit 全局采样产生 enter/exit/press 边沿。翼内容超宽暂不做跑马灯。

use std::time::{Duration, Instant};

use gpui::{
    canvas, div, point, px, rgb, App, AppContext, BorrowAppContext, Bounds, Context,
    InteractiveElement, IntoElement, ParentElement, PathBuilder, Render, SharedString,
    StyleRefinement, Styled, Subscription, Window, WindowBackgroundAppearance, WindowBounds,
    WindowKind, WindowOptions,
};

#[cfg(test)]
use crate::app_state::ConnectionStatus;
use crate::app_state::{AppState, PendingPermission, PendingPrompt, QuestionProgress, SlotContent};
use crate::bridge::HostCommand;
use crate::notch_config::DEFAULT_WIDEN_FACTOR;
#[cfg(target_os = "macos")]
use crate::notch_native as native;
use crate::notch_shape::{self, CARD_CONTENT_WIDTH, CARD_DROP_HEIGHT, CARD_PADDING};
use crate::session::{AgentStatus, SessionState};
use crate::spring::{SpringVal, SPRING_CLOSE, SPRING_OPEN, SPRING_POP};
use notchkit_core::{InteractionController, InteractionInput, InteractionPolicy, PanelMode};

const INTERACTION_POLL_INTERVAL: Duration = Duration::from_millis(16);

/// 形状顶边上溢量（= notch_native::TOP_BLEED，2x 屏 1px）：形状绘制高度 = 逻辑高 + 该值，
/// 顶边出屏 1px 防细缝、底边精确落在真刘海底边。
#[cfg(target_os = "macos")]
const SHAPE_TOP_BLEED: f32 = crate::notch_native::TOP_BLEED as f32;
#[cfg(not(target_os = "macos"))]
const SHAPE_TOP_BLEED: f32 = 0.5;

/// 形状底边向下多盖 0.5pt（2x 屏 = 1px）：用户实测真刘海视觉上还高一点点，
/// 形状底边压到 top_y - bar_h 之下 1px（窗口高度同步加回），贴死不留缝。
const SHAPE_BOTTOM_OVERLAP: f32 = 0.5;

/// 紧凑态默认至少给物理刘海两侧各留 48pt，并保留 280pt 的无刘海视觉下限。
/// AppState.widen_factor 只作为这套响应式基线的开发调参比例，不再直接乘物理刘海宽。
const COMPACT_SIDE_EXTRA: f32 = 96.0;
const COMPACT_MIN_WIDTH: f32 = 280.0;

fn responsive_widened_width(notch_w: f32, tuning_factor: f32) -> f32 {
    let tuning_factor = if tuning_factor > 0.0 {
        tuning_factor.clamp(1.0, 3.0)
    } else {
        DEFAULT_WIDEN_FACTOR
    };
    let scale = tuning_factor / DEFAULT_WIDEN_FACTOR;
    (notch_w + COMPACT_SIDE_EXTRA * scale)
        .max(COMPACT_MIN_WIDTH * scale)
        .max(notch_w)
}

/// 面板目标态（驱动源 = AppState）：
/// - Hidden：无会话/插槽/审批数据——物理刘海本来就是黑的（1x1 透明帧）
/// - Widened：有插槽/活跃会话——刘海行高的黑色连续条，两翼放内容
/// - Dropped：permission_queue 非空——从 Widened 条向下拉出审批卡
///
/// 刘海是独立显示接口：只由内容驱动，与设备连接状态无关。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PanelTarget {
    Hidden,
    Widened,
    Dropped,
}

/// AppState → 应用拥有的基础展示态；交互层可以在其上临时展开。
fn base_mode_for(state: &AppState) -> PanelMode {
    let has_content = !state.sessions.is_empty() || state.slots.iter().any(|s| s.is_some());
    if has_content {
        PanelMode::Compact
    } else {
        PanelMode::Hidden
    }
}

/// 审批是应用强制展开态，优先级高于 hover 和点击固定。
fn forced_mode_for(state: &AppState) -> Option<PanelMode> {
    if !state.permission_queue.is_empty() {
        Some(PanelMode::Expanded)
    } else {
        None
    }
}

fn target_from_mode(mode: PanelMode) -> PanelTarget {
    match mode {
        PanelMode::Hidden => PanelTarget::Hidden,
        PanelMode::Compact => PanelTarget::Widened,
        PanelMode::Expanded => PanelTarget::Dropped,
    }
}

/// 无交互输入时的 AppState → 面板目标态（纯函数回归测试用）。
#[cfg(test)]
fn target_for(state: &AppState) -> PanelTarget {
    target_from_mode(forced_mode_for(state).unwrap_or_else(|| base_mode_for(state)))
}

/// 应答一个权限请求：出队 + 会话状态回到 Processing + 经命令通道完成 oneshot
/// （逻辑与原 approval.rs 一致；队列变化经 AppState observe 自动驱动面板收回）
fn answer(cx: &mut App, req_id: u64, decision: &str) {
    cx.update_global::<AppState, _>(|state, _cx| {
        finish_request(state, req_id);
        if let Some(tx) = &state.host_command_tx {
            let _ = tx.send(HostCommand::AnswerPermission {
                req_id,
                decision: decision.to_string(),
            });
        }
    });
}

fn finish_request(state: &mut AppState, req_id: u64) {
    let Some(position) = state
        .permission_queue
        .iter()
        .position(|request| request.req_id == req_id)
    else {
        return;
    };
    let Some(request) = state.permission_queue.remove(position) else {
        return;
    };
    if let Some(session) = state.sessions.get_mut(&request.session_id) {
        if session.status.is_waiting() {
            session.status = AgentStatus::Processing;
            session.current_tool = None;
        }
    }
}

#[derive(Clone, Copy)]
enum QuestionAction {
    Select(usize),
    Submit,
}

fn answer_question(cx: &mut App, req_id: u64, action: QuestionAction) {
    cx.update_global::<AppState, _>(|state, _cx| {
        let Some(position) = state
            .permission_queue
            .iter()
            .position(|request| request.req_id == req_id)
        else {
            return;
        };
        let progress = {
            let request = &mut state.permission_queue[position];
            match action {
                QuestionAction::Select(option) => request.prompt.select_option(option),
                QuestionAction::Submit => request.prompt.submit_current(),
            }
        };
        let Some(QuestionProgress::Complete(answers)) = progress else {
            return;
        };
        finish_request(state, req_id);
        if let Some(tx) = &state.host_command_tx {
            let _ = tx.send(HostCommand::AnswerQuestions { req_id, answers });
        }
    });
}

pub struct NotchPanelView {
    _state_sub: Subscription,
    /// 公共交互仲裁：应用基础态 < hover/click < 审批强制态。
    interaction: InteractionController,
    #[cfg(target_os = "macos")]
    pointer_tracker: notchkit_macos::PointerTracker,
    #[cfg(target_os = "macos")]
    _interaction_task: gpui::Task<()>,
    #[cfg(target_os = "macos")]
    last_interaction_tick: Instant,
    target: PanelTarget,
    /// 当前包围盒宽（弹簧动画，≥ 物理刘海宽）
    width: SpringVal,
    /// 下拉高度（弹簧动画，0 = 纯 Widened 条）
    drop: SpringVal,
    /// 纵缩完成后才应用的宽度目标（收回序列：先纵缩回两翼、再横缩回刘海）
    pending_width: Option<f32>,
    /// 横扩到位后才应用的下拉目标（展开序列：审批需要多宽上条先扩到多宽，W 不变再下拉）
    pending_drop: Option<f32>,
    last_tick: Option<Instant>,
    animating: bool,
    /// 最近一次派发过的原生窗口尺寸（native_frame 守卫：只在变化时 setFrame，
    /// 动画期间窗口固定，形状在窗口内动）
    last_frame: Option<(f64, f64)>,
    /// 物理刘海宽 / 刘海行高（native 测量值的 f32 缓存，供布局与形状用）
    notch_w: f32,
    bar_h: f32,
    /// 端部样式 / 响应式宽度调参 / 紧凑态反角与圆角半径（AppState 镜像）
    style: notch_shape::EndStyle,
    widen: f32,
    top_r: f32,
    bot_r: f32,
    #[cfg(target_os = "macos")]
    native: Option<native::NativeHandle>,
}

impl NotchPanelView {
    fn new(
        cx: &mut Context<Self>,
        #[cfg(target_os = "macos")] native: Option<native::NativeHandle>,
    ) -> Self {
        #[cfg(target_os = "macos")]
        let (notch_w, bar_h) = native
            .as_ref()
            .map(|n| (n.metrics.notch_w as f32, n.metrics.bar_h as f32))
            .unwrap_or((200.0, 32.0));
        #[cfg(not(target_os = "macos"))]
        let (notch_w, bar_h) = (200.0, 32.0);

        let sub = cx.observe_global::<AppState>(|this, cx| {
            this.on_state_changed(cx);
            cx.notify();
        });

        let base_mode = cx
            .try_global::<AppState>()
            .map(base_mode_for)
            .unwrap_or(PanelMode::Hidden);
        let forced_mode = cx.try_global::<AppState>().and_then(forced_mode_for);
        let mut interaction = InteractionController::new(base_mode, InteractionPolicy::INTERACTIVE);
        interaction.set_forced_mode(forced_mode);

        #[cfg(target_os = "macos")]
        let interaction_task = cx.spawn(async move |this, cx| loop {
            gpui::Timer::after(INTERACTION_POLL_INTERVAL).await;
            if this
                .update(cx, |this, cx| this.poll_interaction(cx))
                .is_err()
            {
                break;
            }
        });

        let mut this = Self {
            _state_sub: sub,
            interaction,
            #[cfg(target_os = "macos")]
            pointer_tracker: notchkit_macos::PointerTracker::new(),
            #[cfg(target_os = "macos")]
            _interaction_task: interaction_task,
            #[cfg(target_os = "macos")]
            last_interaction_tick: Instant::now(),
            target: PanelTarget::Hidden,
            width: SpringVal::settled(notch_w, SPRING_POP),
            drop: SpringVal::settled(0.0, SPRING_OPEN),
            pending_width: None,
            pending_drop: None,
            last_tick: None,
            animating: false,
            last_frame: None,
            notch_w,
            bar_h,
            style: cx
                .try_global::<AppState>()
                .map(|s| s.notch_style)
                .unwrap_or_default(),
            widen: cx
                .try_global::<AppState>()
                .map(|s| s.widen_factor)
                .unwrap_or(crate::notch_config::DEFAULT_WIDEN_FACTOR),
            top_r: cx
                .try_global::<AppState>()
                .map(|s| s.notch_top_r)
                .unwrap_or(crate::notch_config::DEFAULT_TOP_R),
            bot_r: cx
                .try_global::<AppState>()
                .map(|s| s.notch_bot_r)
                .unwrap_or(crate::notch_config::DEFAULT_BOT_R),
            #[cfg(target_os = "macos")]
            native,
        };
        let target = target_from_mode(this.interaction.mode());
        this.apply_target(target);
        this
    }

    /// 目标态 → (宽度目标 W, 下拉高度目标 D)；宽度永远 ≥ 物理刘海宽。
    /// v1 同宽设计：上条与下拉卡永远同宽——审批卡内容需要多宽，W 就扩到多宽；
    /// Capsule 风格额外让出两端半圆（2 * bar_h/2 = bar_h），内容才全程留在黑色形状内。
    /// 默认宽度 = max(物理刘海宽 + 96, 280)；AppState.widen_factor 只缩放这套基线。
    fn targets(&self, t: PanelTarget) -> (f32, f32) {
        let widened = responsive_widened_width(self.notch_w, self.widen);
        match t {
            PanelTarget::Hidden => (self.notch_w, 0.0),
            PanelTarget::Widened => (widened, 0.0),
            PanelTarget::Dropped => {
                let need = CARD_CONTENT_WIDTH + 2.0 * CARD_PADDING;
                // 胶囊端部半圆占宽（r = bar_h/2，比 CARD_PADDING 大）：内容需再整体内缩 r
                // 才能全程留在黑色形状内（否则卡片左右边缘会越出胶囊轮廓）
                let min_w = if self.style == notch_shape::EndStyle::Capsule {
                    need + self.bar_h
                } else {
                    need
                };
                (widened.max(min_w), CARD_DROP_HEIGHT)
            }
        }
    }

    /// 目标切换 → 弹簧重定向。单一形状两参数（W/D）顺序动画：
    /// 展开先横扩（pop/open）到内容宽，W 到位后 D 才下拉（open），下拉期间 W 不变；
    /// 收回先 D 纵缩（close）到 0，到位后 W 才横缩（close）回目标宽度——
    /// 任何中间帧都是一个连续形状。
    fn apply_target(&mut self, t: PanelTarget) {
        let (w_t, d_t) = self.targets(t);
        if d_t > self.drop.target() + 0.01 {
            // 展开：先横扩到内容宽（Hidden 直接进 Dropped 时用 pop 出现），
            // 下拉目标挂起，W 到位后再下拉
            let sp = if self.target == PanelTarget::Hidden {
                SPRING_POP
            } else {
                SPRING_OPEN
            };
            self.width.set_target(w_t, sp);
            self.pending_drop = Some(d_t);
            self.pending_width = None;
        } else if d_t < self.drop.target() - 0.01 {
            // 收回：先纵缩回两翼；宽度目标挂起，纵缩到位后再横缩
            self.drop.set_target(0.0, SPRING_CLOSE);
            self.pending_width = Some(w_t);
            self.pending_drop = None;
        } else if self.drop.is_settled() && self.pending_drop.is_none() {
            // 纯宽度调整
            self.pending_width = None;
            let sp = if w_t < self.width.target() {
                SPRING_CLOSE
            } else {
                SPRING_POP
            };
            self.width.set_target(w_t, sp);
        } else {
            // 纵缩进行中：只更新挂起的宽度目标
            self.pending_width = Some(w_t);
        }
        self.target = t;
        // 激活期窗口固定为最大包围盒（大于当前形状）：非 Dropped 态整窗穿透点击
        //（透明区不挡菜单栏/下方应用），Dropped 态接收点击（审批卡 Allow/Deny）
        #[cfg(target_os = "macos")]
        if let Some(n) = &mut self.native {
            n.set_click_through(t != PanelTarget::Dropped);
        }
        self.animating = true;
        self.last_tick = None;
    }

    /// 全局采样不依赖窗口接收事件，因此紧凑态仍可点击穿透菜单栏，同时支持 hover。
    #[cfg(target_os = "macos")]
    fn poll_interaction(&mut self, cx: &mut Context<Self>) {
        let now = Instant::now();
        let elapsed = now.saturating_duration_since(self.last_interaction_tick);
        self.last_interaction_tick = now;
        self.interaction.step(elapsed);

        let Some(native) = self.native.as_ref() else {
            return;
        };
        let width = self.width.value().max(self.notch_w) as f64;
        let height = (self.bar_h + self.drop.value().max(0.0)) as f64;
        let rect = native.interaction_rect(width, height);
        let Ok(pointer) = self.pointer_tracker.sample(rect) else {
            return;
        };

        if pointer.entered {
            self.interaction.handle(InteractionInput::PointerEntered);
        }
        if pointer.exited {
            self.interaction.handle(InteractionInput::PointerExited);
        }
        if pointer.primary_pressed {
            if pointer.inside {
                let forced = cx
                    .try_global::<AppState>()
                    .and_then(forced_mode_for)
                    .is_some();
                if !forced {
                    self.interaction.handle(InteractionInput::PrimaryClicked);
                }
            } else {
                self.interaction.handle(InteractionInput::OutsideClicked);
            }
        }

        let target = target_from_mode(self.interaction.mode());
        if target != self.target {
            self.apply_target(target);
            cx.notify();
        }
    }

    fn on_state_changed(&mut self, cx: &mut Context<Self>) {
        let Some(state) = cx.try_global::<AppState>() else {
            return;
        };
        // 同步可调配置：样式/半径变化只需重绘；系数变化要重定向宽度弹簧
        self.style = state.notch_style;
        self.top_r = state.notch_top_r;
        self.bot_r = state.notch_bot_r;
        if (state.widen_factor - self.widen).abs() > 0.001 {
            self.widen = state.widen_factor;
            let t = self.target;
            self.apply_target(t);
        }
        self.interaction.set_base_mode(base_mode_for(state));
        self.interaction.set_forced_mode(forced_mode_for(state));
        let target = target_from_mode(self.interaction.mode());
        #[cfg(target_os = "macos")]
        if let Some(n) = &mut self.native {
            if let Err(error) = n.remeasure() {
                log::warn!("notchkit remeasure failed: {error}");
            }
            self.notch_w = n.metrics.notch_w as f32;
            self.bar_h = n.metrics.bar_h as f32;
        }
        if target != self.target {
            self.apply_target(target);
        }
    }

    /// 每帧：推进弹簧 → 应用收回序列 → 同步原生窗口 frame/可见性。
    /// 返回是否仍在动画（决定是否 request_animation_frame 续帧）。
    fn tick(&mut self) -> bool {
        let now = Instant::now();
        let dt = self
            .last_tick
            .map(|t| now.duration_since(t).as_secs_f32())
            .unwrap_or(0.0);
        self.last_tick = Some(now);

        if self.animating && dt > 0.0 {
            self.width.step(dt);
            self.drop.step(dt);
        }
        // 展开序列：横扩接近到位（1.5px 内）就开始下拉——kill latency（Apple：任何人为
        // 等待都是回归；完全吸附的尾部渐进段白白等 ~0.2s）。下拉期间 W 只再爬 1.5px，
        // 内容宽差不可感知。
        if let Some(d) = self.pending_drop {
            if (self.width.target() - self.width.value()).abs() < 1.5
                && self.width.velocity().abs() < 80.0
            {
                self.drop.set_target(d, SPRING_OPEN);
                self.pending_drop = None;
            }
        }
        // 收回序列：纵缩接近到位就开始横缩（对称镜像，避免尾部延迟）
        if let Some(w) = self.pending_width {
            if (self.drop.target() - self.drop.value()).abs() < 1.5
                && self.drop.velocity().abs() < 80.0
            {
                self.width.set_target(w, SPRING_CLOSE);
                self.pending_width = None;
            }
        }
        self.animating = !(self.width.is_settled()
            && self.drop.is_settled()
            && self.pending_width.is_none()
            && self.pending_drop.is_none());

        self.animating
    }

    /// 当前帧应呈现的原生窗口包围盒：激活期尺寸固定（不再逐帧 resize——每帧
    /// setFrame → CAMetalLayer setDrawableSize 丢弃内容 → 整窗闪动），形状在窗口内
    /// 由弹簧动画驱动（见 render/panel_shape）。
    /// - Hidden 稳定态 = 1x1 透明帧钉在刘海位（一个像素都不画——物理刘海本来就是黑的；
    ///   但不能 orderOut——隐藏窗口收不到 drawRect，状态变化唤不醒渲染循环，会永远卡死）
    /// - 激活 = (最大宽, 状态高)：最大宽 = Dropped 目标宽（各状态宽度最大值）；
    ///   下拉展开/收回进行中用全高，否则条高
    fn native_frame(&self) -> (f64, f64) {
        if self.target == PanelTarget::Hidden && !self.animating {
            (1.0, 1.0)
        } else {
            let (max_w, _) = self.targets(PanelTarget::Dropped);
            let h = if self.target == PanelTarget::Dropped || self.drop.value() > 0.5 {
                self.bar_h + CARD_DROP_HEIGHT
            } else {
                self.bar_h
            };
            // 窗口高度加回底边 overlap：形状底边才能压到真刘海底边之下 1px
            (max_w as f64, (h + SHAPE_BOTTOM_OVERLAP) as f64)
        }
    }

    // ---- 内容渲染（两翼 + 刘海区占位 + 下拉卡），逻辑沿用原 notch.rs / approval.rs ----

    /// 刘海区占位内容：正对系统物理刘海挖孔——屏上永远看不到（被挖孔盖住），
    /// 但截图（framebuffer）能拍到。放 logo/水印/彩蛋都行，换这里即可。
    fn notch_area_content() -> impl IntoElement {
        div()
            .text_size(px(11.0))
            .text_color(rgb(0x45475a))
            .child(SharedString::from("K9"))
    }

    fn render_slot(slot: usize, content: &SlotContent) -> impl IntoElement {
        let label = match slot {
            1 => "Vol",
            2 => "Subs",
            3 => "AI",
            _ => "",
        };
        let text: SharedString = match content {
            SlotContent::Text(t) => t.clone().into(),
            SlotContent::Numeric(v) => format!("{label} {v}").into(),
            SlotContent::Progress(p) => format!("{label} {p}%").into(),
        };
        div()
            .px(px(6.0))
            .text_size(px(12.0))
            .text_color(rgb(0xcdd6f4))
            .child(text)
    }

    /// 选出最高优先级活跃会话：waitingApproval > waitingQuestion > running > processing，
    /// 同级取最近活动；Idle 仍显示，直到看门狗移除会话。
    fn top_session(state: &AppState) -> Option<&SessionState> {
        state.sessions.values().max_by(|a, b| {
            a.status
                .priority()
                .cmp(&b.status.priority())
                .then(a.last_activity.cmp(&b.last_activity))
        })
    }

    /// 会话状态区文本与颜色：`source: 状态/工具`，waitingApproval 黄色警示
    fn render_session(sess: &SessionState) -> impl IntoElement {
        let mut source = sess.source.clone();
        if let Some(c) = source.get_mut(0..1) {
            c.make_ascii_uppercase();
        }
        let (text, color) = match sess.status {
            AgentStatus::WaitingApproval => {
                let tool = sess.current_tool.as_deref().unwrap_or("approval");
                (format!("! {source}: {tool}"), 0xf9e2af)
            }
            AgentStatus::WaitingQuestion => (format!("? {source}: choose option"), 0xf9e2af),
            AgentStatus::Running => {
                let tool = sess.current_tool.as_deref().unwrap_or("running");
                (format!("{source}: {tool}"), 0xa6e3a1)
            }
            AgentStatus::Processing => (format!("{source}: working"), 0x89b4fa),
            AgentStatus::Idle => (format!("{source}: idle"), 0x6c7086),
        };
        div()
            .text_size(px(12.0))
            .text_color(rgb(color))
            .whitespace_nowrap()
            .child(SharedString::from(text))
    }

    fn decision_button(
        req_id: u64,
        label: &'static str,
        decision: &'static str,
        color: u32,
    ) -> impl IntoElement {
        div()
            .px(px(20.0))
            .py(px(6.0))
            .rounded(px(6.0))
            .bg(rgb(0x45475a))
            .text_color(rgb(color))
            .cursor_pointer()
            .hover(|s: StyleRefinement| s.bg(rgb(0x585b70)))
            .child(SharedString::from(label))
            .on_mouse_down(
                gpui::MouseButton::Left,
                move |_ev: &gpui::MouseDownEvent, _window: &mut Window, cx: &mut App| {
                    answer(cx, req_id, decision);
                },
            )
    }

    /// 普通权限卡：source 徽标/工具名/摘要/Allow/Deny。
    fn render_approval_card(p: &PendingPermission) -> impl IntoElement {
        let source_badge = div()
            .px(px(8.0))
            .py(px(2.0))
            .rounded(px(4.0))
            .bg(rgb(0x45475a))
            .text_size(px(11.0))
            .text_color(rgb(0xb4befe))
            .child(SharedString::from(p.source.to_uppercase()));

        let tool_name =
            div()
                .text_size(px(14.0))
                .text_color(rgb(0xfab387))
                .child(SharedString::from(if p.tool_name.is_empty() {
                    "Permission".to_string()
                } else {
                    p.tool_name.clone()
                }));

        let summary_text = if p.tool_name == "Bash" {
            format!("$ {}", p.summary)
        } else {
            p.summary.clone()
        };
        let summary = div()
            .text_size(px(13.0))
            .text_color(rgb(0xa6adc8))
            .whitespace_nowrap()
            .child(SharedString::from(summary_text));

        let req_id = p.req_id;
        let buttons = div()
            .flex()
            .flex_row()
            .gap(px(12.0))
            .child(Self::decision_button(req_id, "Allow", "allow", 0xa6e3a1))
            .child(Self::decision_button(req_id, "Deny", "deny", 0xf38ba8));

        div()
            .flex()
            .flex_col()
            .gap(px(8.0))
            .text_color(rgb(0xcdd6f4))
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap(px(10.0))
                    .child(source_badge)
                    .child(tool_name),
            )
            .child(summary)
            .child(buttons)
    }

    fn question_option_button(
        req_id: u64,
        option_index: usize,
        label: String,
        selected: bool,
    ) -> impl IntoElement {
        let background = if selected { 0x313244 } else { 0x45475a };
        let color = if selected { 0xa6e3a1 } else { 0xcdd6f4 };
        div()
            .px(px(10.0))
            .py(px(5.0))
            .rounded(px(6.0))
            .bg(rgb(background))
            .text_size(px(12.0))
            .text_color(rgb(color))
            .cursor_pointer()
            .hover(|style: StyleRefinement| style.bg(rgb(0x585b70)))
            .child(SharedString::from(label))
            .on_mouse_down(
                gpui::MouseButton::Left,
                move |_event: &gpui::MouseDownEvent, _window: &mut Window, cx: &mut App| {
                    answer_question(cx, req_id, QuestionAction::Select(option_index));
                },
            )
    }

    fn render_question_card(p: &PendingPermission) -> impl IntoElement {
        let Some((question, current, total)) = p.prompt.current_question() else {
            return div().child("Invalid question payload");
        };
        let mut options = div().flex().flex_row().flex_wrap().gap(px(6.0));
        for (index, option) in question.options.iter().enumerate() {
            let label = match option.description.as_deref() {
                Some(description) if !description.is_empty() => {
                    format!("{} · {}", option.label, description)
                }
                _ => option.label.clone(),
            };
            options = options.child(Self::question_option_button(
                p.req_id,
                index,
                label,
                question.selected.contains(&index),
            ));
        }

        let mut controls = div().flex().flex_row().items_center().gap(px(8.0));
        if question.multi_select && !question.selected.is_empty() {
            let req_id = p.req_id;
            controls = controls.child(
                div()
                    .px(px(12.0))
                    .py(px(5.0))
                    .rounded(px(6.0))
                    .bg(rgb(0x45475a))
                    .text_size(px(12.0))
                    .text_color(rgb(0xa6e3a1))
                    .cursor_pointer()
                    .child("Submit")
                    .on_mouse_down(
                        gpui::MouseButton::Left,
                        move |_event: &gpui::MouseDownEvent, _window: &mut Window, cx: &mut App| {
                            answer_question(cx, req_id, QuestionAction::Submit);
                        },
                    ),
            );
        }
        controls = controls.child(Self::decision_button(p.req_id, "Cancel", "deny", 0xf38ba8));

        div()
            .flex()
            .flex_col()
            .gap(px(6.0))
            .text_color(rgb(0xcdd6f4))
            .child(
                div()
                    .text_size(px(11.0))
                    .text_color(rgb(0xb4befe))
                    .child(SharedString::from(format!(
                        "QUESTION {}/{}{}",
                        current + 1,
                        total,
                        if question.multi_select {
                            " · MULTI"
                        } else {
                            ""
                        }
                    ))),
            )
            .child(
                div()
                    .text_size(px(13.0))
                    .text_color(rgb(0xfab387))
                    .child(SharedString::from(question.prompt.clone())),
            )
            .child(options)
            .child(controls)
    }

    fn render_request_card(p: &PendingPermission) -> gpui::AnyElement {
        match &p.prompt {
            PendingPrompt::Approval => Self::render_approval_card(p).into_any_element(),
            PendingPrompt::Questions { .. } => Self::render_question_card(p).into_any_element(),
        }
    }

    /// K9 自己注入的普通展开内容；公共库只决定何时展开，不理解这些业务字段。
    fn render_dashboard(state: &AppState, pinned: bool) -> impl IntoElement {
        let connection: SharedString = match &state.connection {
            crate::app_state::ConnectionStatus::Disconnected => "Keyboard disconnected".into(),
            crate::app_state::ConnectionStatus::Connecting => "Connecting to keyboard…".into(),
            crate::app_state::ConnectionStatus::Connected => "Keyboard connected".into(),
            crate::app_state::ConnectionStatus::Error(error) => {
                format!("Keyboard error: {error}").into()
            }
        };
        let active_sessions = state
            .sessions
            .values()
            .filter(|session| session.status != AgentStatus::Idle)
            .count();
        let populated_slots = state.slots.iter().filter(|slot| slot.is_some()).count();
        let summary: SharedString =
            format!("{active_sessions} active sessions · {populated_slots} live slots").into();
        let hint: SharedString = if pinned {
            "Pinned · click again or click outside to release".into()
        } else {
            "Hover preview · click to pin".into()
        };

        div()
            .flex()
            .flex_col()
            .gap(px(8.0))
            .text_color(rgb(0xcdd6f4))
            .child(div().text_size(px(14.0)).child(connection))
            .child(
                div()
                    .text_size(px(12.0))
                    .text_color(rgb(0xa6adc8))
                    .child(summary),
            )
            .child(
                div()
                    .text_size(px(11.0))
                    .text_color(rgb(0x89b4fa))
                    .child(hint),
            )
    }
}

impl Render for NotchPanelView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let animating = self.tick();
        if animating {
            // 续帧：下一帧 notify 本 view，形成逐帧动画循环
            window.request_animation_frame();
        }

        // 原生 frame：只在尺寸变化时 dispatch 到主队列下一 tick（每帧 setFrame →
        // CAMetalLayer setDrawableSize 丢弃内容 → 整窗闪动；故激活期窗口固定，
        // 形状在窗口内由弹簧动画驱动，见 native_frame/panel_shape 注释）
        #[cfg(target_os = "macos")]
        if let Some(n) = &self.native {
            let (w, h) = self.native_frame();
            if self.last_frame != Some((w, h)) {
                n.set_frame_deferred(w, h);
                self.last_frame = Some((w, h));
            }
        }

        // Hidden 稳定态：什么都不画（窗口是 1x1 透明帧）
        if self.target == PanelTarget::Hidden && !animating {
            return div().size_full();
        }

        let bar_h = self.bar_h;
        // 形状（弹簧驱动）在固定尺寸窗口内水平居中；窗口宽 = native_frame 的目标宽
        let shape_w = self.width.value();
        let shape_h = bar_h + self.drop.value();
        let (win_w, _) = self.native_frame();
        let xoff = ((win_w as f32 - shape_w) / 2.0).max(0.0);

        let mut root = div().size_full().relative().child(panel_shape(
            bar_h,
            self.style,
            self.top_r,
            self.bot_r,
            shape_w,
            shape_h + SHAPE_TOP_BLEED + SHAPE_BOTTOM_OVERLAP,
        ));

        if let Some(state) = cx.try_global::<AppState>() {
            // 中间正对系统刘海区：占位 View 钉住——两翼内容由下方 wing_w 数学天然进不来，
            // 这个 View 是显式占据（截图可见 / 屏上被挖孔盖住），内容见 notch_area_content
            let notch_left = xoff + (shape_w - self.notch_w) / 2.0;
            root = root.child(
                div()
                    .absolute()
                    .left(px(notch_left))
                    .top_0()
                    .w(px(self.notch_w))
                    .h(px(bar_h))
                    .flex()
                    .items_center()
                    .justify_center()
                    .overflow_hidden()
                    .child(Self::notch_area_content()),
            );

            // 两翼区宽 = (形状宽 - 物理刘海宽)/2；中间正对刘海区留空（刘海本体不显示内容）
            let wing_w = ((shape_w - self.notch_w) / 2.0).max(0.0);
            if wing_w > 24.0 {
                // 左翼：左对齐（从形状左缘 xoff 起），最高优先级会话 `source: 状态/工具`（或连接状态文本）
                let mut left = div()
                    .absolute()
                    .left(px(xoff))
                    .top_0()
                    .h(px(bar_h))
                    .w(px(wing_w))
                    .flex()
                    .flex_row()
                    .items_center()
                    .pl(px(10.0))
                    .overflow_hidden();
                if let Some(sess) = Self::top_session(state) {
                    left = left.child(Self::render_session(sess));
                }

                // 右翼：右对齐，slot 镜像 + 弹窗结果（不显示连接状态——刘海是独立显示接口）
                let mut right = div()
                    .absolute()
                    .right(px(xoff))
                    .top_0()
                    .h(px(bar_h))
                    .w(px(wing_w))
                    .flex()
                    .flex_row()
                    .items_center()
                    .justify_end()
                    .pr(px(10.0))
                    .overflow_hidden();

                for (i, slot) in state.slots.iter().enumerate() {
                    if let Some(content) = slot {
                        right = right.child(Self::render_slot(i, content));
                    }
                }
                if let Some((id, result)) = &state.last_dialog_result {
                    right = right.child(
                        div()
                            .px(px(6.0))
                            .text_size(px(11.0))
                            .text_color(rgb(0x89b4fa))
                            .whitespace_nowrap()
                            .child(SharedString::from(format!("#{id} {result:?}"))),
                    );
                }

                root = root.child(left).child(right);
            }

            // 下拉卡：与上条同宽，内容居中限宽 CARD_CONTENT_WIDTH
            //（纵缩过程中队列可能已空 → 只留黑卡体）；高度随形状（黑体）同步，
            // 露出边缘永远领先黑体底边固定 4pt，与黑体同帧不闪
            let drop_h = (shape_h - bar_h).max(0.0);
            if drop_h > 1.0 {
                let content_w = CARD_CONTENT_WIDTH.min(shape_w - 2.0 * CARD_PADDING);
                let content = match state.permission_queue.front() {
                    Some(permission) => Self::render_request_card(permission),
                    None => Self::render_dashboard(state, self.interaction.is_pinned())
                        .into_any_element(),
                };
                root = root.child(
                    div()
                        .absolute()
                        .left(px(xoff + (shape_w - content_w) / 2.0))
                        .top(px(bar_h + 4.0))
                        .w(px(content_w))
                        .h(px((drop_h - 8.0).max(0.0)))
                        .overflow_hidden()
                        .child(content),
                );
            }
        }

        root
    }
}

/// 黑色连续形状（canvas 填充）：轮廓由 notch_shape::outline 按「弹簧形状尺寸」生成——
/// 窗口激活期尺寸固定，形状在窗口内水平居中（xoff），高度 = 逻辑高 + 上溢 1px
/// （顶边出屏防细缝、底边精确落在真刘海底边）。任意时刻都是一个连续形状（同宽设计）；
/// k=0.62 连续曲率底角 div 圆角画不了，必须用 path。
fn panel_shape(
    bar_h: f32,
    style: notch_shape::EndStyle,
    top_r: f32,
    bot_r: f32,
    shape_w: f32,
    shape_h: f32,
) -> impl IntoElement {
    canvas(
        |_, _, _| (),
        move |bounds, _, window, _| {
            let cw: f32 = bounds.size.width.into();
            let xoff = ((cw - shape_w) / 2.0).max(0.0);
            let ox = bounds.origin.x + px(xoff);
            let oy = bounds.origin.y;
            let mut b = PathBuilder::fill();
            b.move_to(point(ox, oy));
            for seg in notch_shape::outline_r(style, shape_w, shape_h, bar_h, top_r, bot_r) {
                match seg {
                    notch_shape::Seg::Line(x, y) => {
                        b.line_to(point(ox + px(x), oy + px(y)));
                    }
                    notch_shape::Seg::Cubic(x1, y1, x2, y2, x, y) => {
                        b.cubic_bezier_to(
                            point(ox + px(x), oy + px(y)),
                            point(ox + px(x1), oy + px(y1)),
                            point(ox + px(x2), oy + px(y2)),
                        );
                    }
                }
            }
            b.close();
            if let Ok(path) = b.build() {
                window.paint_path(path, rgb(0x000000));
            }
        },
    )
    .absolute()
    .size_full()
}

/// 打开刘海面板窗口：单个 PopUp 透明窗（无阴影/无 titlebar/不可移动），
/// 初始帧很小，open 回调里立刻由 native 层测量并钉到刘海行
pub fn open_notch_panel_window(app: &mut App) {
    let bounds = match app.primary_display() {
        Some(display) => {
            let db = display.bounds();
            Bounds::new(
                gpui::point(db.origin.x + (db.size.width - px(220.0)) / 2.0, db.origin.y),
                gpui::size(px(220.0), px(2.0)),
            )
        }
        None => Bounds::new(
            gpui::point(px(200.0), px(0.0)),
            gpui::size(px(220.0), px(2.0)),
        ),
    };

    let options = WindowOptions {
        titlebar: None,
        window_bounds: Some(WindowBounds::Windowed(bounds)),
        kind: WindowKind::PopUp,
        is_movable: false,
        is_resizable: false,
        focus: false,
        show: true,
        window_background: WindowBackgroundAppearance::Transparent,
        ..Default::default()
    };

    app.open_window(options, |window, cx| {
        #[cfg(target_os = "macos")]
        let native = native::prepare(window);
        cx.new(|cx| {
            NotchPanelView::new(
                cx,
                #[cfg(target_os = "macos")]
                native,
            )
        })
    })
    .unwrap();
}

#[cfg(test)]
mod tests {
    use super::*;

    fn state_with_pending() -> AppState {
        let mut s = AppState::default();
        s.permission_queue.push_back(PendingPermission {
            req_id: 1,
            session_id: "s1".into(),
            source: "claude".into(),
            tool_name: "Bash".into(),
            summary: "ls".into(),
            prompt: PendingPrompt::Approval,
            tool_use_id: None,
        });
        s
    }

    #[test]
    fn target_state_machine() {
        // 空状态 → Hidden
        assert_eq!(target_for(&AppState::default()), PanelTarget::Hidden);
        // 有 pending → Dropped（最高优先级）
        assert_eq!(target_for(&state_with_pending()), PanelTarget::Dropped);
        // 仅连接状态（无内容）→ Hidden（刘海与设备连接解耦）
        let mut s = AppState::default();
        s.connection = ConnectionStatus::Connecting;
        assert_eq!(target_for(&s), PanelTarget::Hidden);
        // 已登记但空闲的 agent 会话 → Widened，明确显示 idle
        let mut s = AppState::default();
        s.sessions.insert(
            "idle".into(),
            SessionState::new("idle".into(), "claude".into()),
        );
        assert_eq!(target_for(&s), PanelTarget::Widened);
        // 有 slot 数据 → Widened
        let mut s = AppState::default();
        s.slots[0] = Some(SlotContent::Text("12:00".into()));
        assert_eq!(target_for(&s), PanelTarget::Widened);
        // 有 pending 且有连接 → 仍 Dropped
        let mut s = state_with_pending();
        s.connection = ConnectionStatus::Connected;
        assert_eq!(target_for(&s), PanelTarget::Dropped);
    }

    #[test]
    fn width_targets_never_below_notch() {
        // targets 的宽度公式（与 NotchPanelView::targets 同源）：任何形态 W ≥ 物理刘海宽
        let notch_w = 200.0_f32;
        let widened = responsive_widened_width(notch_w, DEFAULT_WIDEN_FACTOR);
        assert!(widened >= notch_w);
        assert!((widened - 296.0).abs() < 0.01);
        let need = CARD_CONTENT_WIDTH + 2.0 * CARD_PADDING;
        assert!((need - 376.0).abs() < 0.01);
        assert!(widened.max(need) >= notch_w);
        // 同宽设计：下拉目标宽 = max(Widened 宽, 内容宽 + 两侧内边距)
        let dropped_w = widened.max(need);
        assert!((dropped_w - 376.0).abs() < 0.01);
        assert!(dropped_w >= need || (dropped_w - widened).abs() < 0.01);
        // Capsule 风格：端部半圆占宽 bar_h（r = bar_h/2），内容需再内缩 r 才不越出黑色形状
        let bar_h = 32.0_f32;
        let capsule_w = widened.max(need + bar_h);
        assert!(capsule_w >= need + bar_h);
    }

    #[test]
    fn responsive_width_tuning_preserves_notch_floor() {
        let notch_w = 240.0_f32;
        let narrow = responsive_widened_width(notch_w, 1.0);
        let default = responsive_widened_width(notch_w, DEFAULT_WIDEN_FACTOR);
        let wide = responsive_widened_width(notch_w, 3.0);
        assert_eq!(responsive_widened_width(notch_w, 0.0), default);
        assert!(narrow >= notch_w);
        assert!(narrow < default && default < wide);
        assert!((default - 336.0).abs() < 0.01);
    }
}
