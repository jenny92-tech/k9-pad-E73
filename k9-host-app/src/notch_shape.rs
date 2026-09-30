// INPUT:  notchkit-core（公共刘海组件的 renderer-independent geometry）、std
// OUTPUT: K9 既有 EndStyle/Seg/参数接口到 notchkit-core Outline 的薄适配层与回归测试
// POS:    GPUI 形状兼容层 — 几何实现由独立 notchkit-rs 仓库统一维护；v1 同宽设计：
//         任意时刻轮廓只有一个黑色连续形状，由 W（宽）/ D（下拉高）两个动画参数驱动；
//         顶边两角直角贴屏幕顶（与物理刘海一致）；D=0 时两端底部外角圆角（曲率风格同原刘海）；
//         D>0 时侧边直线垂到卡底，底角 k=0.62 连续曲率 cubic 近似，顶部反角与底部圆角
//         随 D 从紧凑态 6/14 连续过渡到展开态 12/22（收回全程圆润，不出现直角）；
//         Capsule 风格全程同形：端部半圆 r=bar_h/2 恒定，D>0 只是条长高（Dynamic Island 手感）
//         （蓝本 NotchPanelView.swift:2631-2670 的 bottomRadius 段，control 偏移 br*(1-k)）

/// 下拉卡完全展开时的顶部反角/底部圆角（32pt 行高基准）。
/// 高卡片需要比紧凑条更柔和的曲率，随下拉进度连续过渡，避免形状只是机械地“长高”。
pub const EXPANDED_TOP_RADIUS: f32 = 12.0;
pub const BOTTOM_RADIUS: f32 = 22.0;
/// 连续曲率系数：蓝本 k=0.62（0.5523=正圆，0.62 更紧，Apple squircle 手感），
/// cubic control 偏移 = r*(1-k)
pub const CORNER_K: f32 = 0.62;
/// Widened 条两端外侧圆角 = bar_h * 0.45（近似值：与物理刘海底部圆角风格一致，
/// 物理刘海圆角无公开 API，按视觉比例取行高 45%，并钳制到 width/4）
pub const END_RADIUS_RATIO: f32 = 0.45;
/// 下拉卡内容宽度（pt）；W 目标 = max(Widened 宽, CARD_CONTENT_WIDTH + 2*CARD_PADDING)
pub const CARD_CONTENT_WIDTH: f32 = 340.0;
/// 下拉卡内容水平内边距（pt）
pub const CARD_PADDING: f32 = 18.0;
/// 下拉目标高度（pt，bar_h 之下）：内容（徽标行/摘要/按钮）+ 上下留白
pub const CARD_DROP_HEIGHT: f32 = 126.0;

/// 轮廓线段：直线到 (x,y) 或三次贝塞尔到 (x,y)（control1, control2）
/// 坐标系：窗口内容坐标，左上角原点，y 向下
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Seg {
    Line(f32, f32),
    Cubic(f32, f32, f32, f32, f32, f32),
}

/// 端部样式（`~/.k9pad/config.json` 的 `notch_style` 选择）
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum EndStyle {
    /// 原刘海风格（默认）：端部上边直角贴屏幕边缘、下边圆弧——「刘海变宽」
    #[default]
    Notch,
    /// 胶囊：端部上下全圆（r = bar_h/2 半圆端）
    Capsule,
}

impl EndStyle {
    pub fn from_config_str(s: &str) -> Self {
        match s {
            "capsule" => Self::Capsule,
            _ => Self::Notch,
        }
    }
}

/// Widened 条端部圆角半径（近似物理刘海底部圆角，钳制到 width/4 防止动画中变形）
pub fn end_radius(bar_h: f32, width: f32) -> f32 {
    (bar_h * END_RADIUS_RATIO).min(width / 4.0).max(0.0)
}

/// 下拉卡底角半径：全展开 = BOTTOM_RADIUS，随下拉量从上条端部圆角 bot_r 线性过渡。
/// 旧版按 drop 钳制（`.min(drop)`）会在收回时把半径压成 0（直角）再弹回圆弧；
/// 改为连续过渡后全程保持圆角，且 drop=0 时恰好等于上条端部圆角，与 Widened 形态无缝衔接。
/// 仍钳制到 width/4 防止窄条变形。
pub fn card_bottom_radius(width: f32, drop: f32, bar_bot_r: f32) -> f32 {
    let t = (drop / CARD_DROP_HEIGHT).clamp(0.0, 1.0);
    (bar_bot_r + (BOTTOM_RADIUS - bar_bot_r) * t)
        .min(width / 4.0)
        .max(0.0)
}

/// 生成当前帧的完整轮廓（顺时针，从左上 (0,0) 开始）——任意时刻都是一个连续形状：
/// - Notch 风格（默认）：1:1 复刻真实 MacBook 刘海（boring.notch/DynamicNotchKit 的
///   NotchShape.swift）——紧凑态顶边两角 r=6 内凹反角连接屏幕边缘、底部两角 r=14 外凸圆角，
///   Dropped 时连续过渡到 12/22（均按 32pt 基准行高等比缩放）；
/// - Capsule 风格：端部上下全圆（r = bar_h/2 半圆端）。
/// 二次贝塞尔转三次：C1 = P0 + 2/3(C-P0)，C2 = P1 + 2/3(C-P1)。
pub fn outline(style: EndStyle, width: f32, height: f32, bar_h: f32) -> Vec<Seg> {
    outline_r(style, width, height, bar_h, 6.0, 14.0)
}

/// 带自定义半径的轮廓生成：`top_r_base`/`bot_r_base` 是 32pt 行高基准值，
/// 运行时按实际行高等比缩放
pub fn outline_r(
    style: EndStyle,
    width: f32,
    height: f32,
    bar_h: f32,
    top_r_base: f32,
    bot_r_base: f32,
) -> Vec<Seg> {
    let scale = (bar_h / 32.0).max(0.5);
    let spec = notchkit_core::ShapeSpec {
        compact_top_radius: top_r_base,
        compact_bottom_radius: bot_r_base,
        expanded_top_radius: EXPANDED_TOP_RADIUS,
        // notchkit-core 的展开半径会按 bar_h 缩放；这里反向归一化，保持 K9 现有
        // “展开底角固定 22pt、紧凑底角随刘海行高缩放”的视觉参数完全不变。
        expanded_bottom_radius: BOTTOM_RADIUS / scale,
        expanded_drop_height: CARD_DROP_HEIGHT,
        continuous_corner_factor: CORNER_K,
    };
    let core_style = match style {
        EndStyle::Notch => notchkit_core::ShapeStyle::Notch,
        EndStyle::Capsule => notchkit_core::ShapeStyle::Capsule,
    };
    let mut segments = notchkit_core::outline(spec, core_style, width, height, bar_h).segments;
    // Core 的 Outline 显式闭合；K9 的 GPUI PathBuilder 在调用方 close()，保留原先
    // capsule 的 8 段约定，避免多画一条等价的顶边闭合线。
    if style == EndStyle::Capsule
        && matches!(
            segments.last(),
            Some(notchkit_core::PathSegment::LineTo(point))
                if point.x.abs() < f32::EPSILON && point.y.abs() < f32::EPSILON
        )
    {
        segments.pop();
    }
    segments
        .into_iter()
        .map(|segment| match segment {
            notchkit_core::PathSegment::LineTo(point) => Seg::Line(point.x, point.y),
            notchkit_core::PathSegment::CubicTo {
                control_1,
                control_2,
                end,
            } => Seg::Cubic(
                control_1.x,
                control_1.y,
                control_2.x,
                control_2.y,
                end.x,
                end.y,
            ),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_finite_and_in_bounds(segs: &[Seg], w: f32, h: f32) {
        for s in segs {
            let pts: Vec<(f32, f32)> = match *s {
                Seg::Line(x, y) => vec![(x, y)],
                Seg::Cubic(x1, y1, x2, y2, x, y) => vec![(x1, y1), (x2, y2), (x, y)],
            };
            for (x, y) in pts {
                assert!(x.is_finite() && y.is_finite(), "非有限坐标 in {s:?}");
                assert!((-0.01..=w + 0.01).contains(&x), "x 越界 {x} (w={w})");
                assert!((-0.01..=h + 0.01).contains(&y), "y 越界 {y} (h={h})");
            }
        }
    }

    /// bar_h=32 时的基准半径（缩放因子 s=1）
    const BASE_TOP_R: f32 = 6.0;
    const BASE_BOT_R: f32 = 14.0;

    #[test]
    fn widened_bar_true_notch_shape() {
        // 真实刘海形状：8 段（顶反角/左侧边/左下圆角/底边/右下圆角/右侧边/顶反角/闭合）
        let segs = outline(EndStyle::Notch, 360.0, 32.0, 32.0);
        assert_finite_and_in_bounds(&segs, 360.0, 32.0);
        assert_eq!(segs.len(), 8);
        // 首段：左上内凹反角，quad→cubic 后终点 (top_r, top_r)
        assert!(matches!(segs[0], Seg::Cubic(_, _, _, _, x, y)
            if (x - BASE_TOP_R).abs() < 0.01 && (y - BASE_TOP_R).abs() < 0.01));
        // 反角的两个控制点：c1=(2/3 top_r, 0) c2=(top_r, 1/3 top_r)（贴顶边内凹）
        assert!(matches!(segs[0], Seg::Cubic(x1, y1, x2, y2, _, _)
            if (x1 - 2.0/3.0*BASE_TOP_R).abs() < 0.01 && y1.abs() < 0.01
            && (x2 - BASE_TOP_R).abs() < 0.01 && (y2 - BASE_TOP_R/3.0).abs() < 0.01));
        // 左侧边在 x=top_r（顶部外扩形成「挂耳」，比身体宽 top_r）
        assert!(matches!(segs[1], Seg::Line(x, y)
            if (x - BASE_TOP_R).abs() < 0.01 && (y - (32.0 - BASE_BOT_R)).abs() < 0.01));
        // 最底点 = bar_h（不下拉）
        let max_y = segs
            .iter()
            .map(|s| match *s {
                Seg::Line(_, y) => y,
                Seg::Cubic(_, _, _, _, _, y) => y,
            })
            .fold(0.0_f32, f32::max);
        assert!((max_y - 32.0).abs() < 0.01);
    }

    #[test]
    fn dropped_same_width_straight_sides() {
        let (w, bar_h) = (376.0, 32.0);
        let h = bar_h + CARD_DROP_HEIGHT;
        let segs = outline(EndStyle::Notch, w, h, bar_h);
        assert_finite_and_in_bounds(&segs, w, h);
        assert_eq!(segs.len(), 8);
        // 侧边一条直线：左侧边从 (top_r, h-br) 直上（第一段反角之后）
        let br = card_bottom_radius(w, CARD_DROP_HEIGHT, BASE_BOT_R);
        assert!(matches!(segs[1], Seg::Line(x, y)
            if (x - EXPANDED_TOP_RADIUS).abs() < 0.01 && (y - (h - br)).abs() < 0.01));
        // 最底点 = 包围盒底
        let max_y = segs
            .iter()
            .map(|s| match *s {
                Seg::Line(_, y) => y,
                Seg::Cubic(_, _, _, _, _, y) => y,
            })
            .fold(0.0_f32, f32::max);
        assert!((max_y - h).abs() < 0.01);
    }

    #[test]
    fn bottom_corner_quad_to_cubic_conversion() {
        // 左下外凸角：quad(P0=(top_r,h-br), C=(top_r,h), P1=(top_r+br,h)) 转 cubic 后
        // c1=(top_r, h-br/3)，c2=(top_r+br/3, h)
        let (w, bar_h) = (376.0, 32.0);
        let h = bar_h + CARD_DROP_HEIGHT;
        let br = card_bottom_radius(w, CARD_DROP_HEIGHT, BASE_BOT_R);
        let segs = outline(EndStyle::Notch, w, h, bar_h);
        assert!(segs.iter().any(|s| matches!(*s,
            Seg::Cubic(x1, y1, x2, y2, x, y)
                if (x1 - EXPANDED_TOP_RADIUS).abs() < 0.01
                && (y1 - (h - br / 3.0)).abs() < 0.01
                && (x2 - (EXPANDED_TOP_RADIUS + br / 3.0)).abs() < 0.01
                && (y2 - h).abs() < 0.01
                && (x - (EXPANDED_TOP_RADIUS + br)).abs() < 0.01
                && (y - h).abs() < 0.01)));
    }

    #[test]
    fn capsule_rounds_all_end_corners() {
        // 胶囊：8 段，右端上半圆角终点 (w, r)（r = bar_h/2）
        let segs = outline(EndStyle::Capsule, 360.0, 32.0, 32.0);
        assert_finite_and_in_bounds(&segs, 360.0, 32.0);
        assert_eq!(segs.len(), 8);
        assert!(matches!(segs[1], Seg::Cubic(_, _, _, _, x, y)
            if (x - 360.0).abs() < 0.01 && (y - 16.0).abs() < 0.01));
    }

    #[test]
    fn capsule_drop_keeps_capsule_geometry() {
        // 胶囊下拉全程同形：端部半圆 r = bar_h/2 恒定、侧边直线、上下边直边——
        // 任意中间帧都是胶囊（Dynamic Island 手感），不会退回刘海几何
        let (w, bar_h) = (400.0, 32.0);
        let r = bar_h / 2.0;
        for drop in [0.0, 1.0, 12.5, CARD_DROP_HEIGHT] {
            let h = bar_h + drop;
            let segs = outline(EndStyle::Capsule, w, h, bar_h);
            assert_finite_and_in_bounds(&segs, w, h);
            assert_eq!(segs.len(), 8);
            // 右端半圆角终点恒在 (w, r)，与下拉量无关
            assert!(matches!(segs[1], Seg::Cubic(_, _, _, _, x, y)
                if (x - w).abs() < 0.01 && (y - r).abs() < 0.01));
            // 左侧边直线端点恒在 (0, r)
            assert!(matches!(segs[6], Seg::Line(x, y)
                if x.abs() < 0.01 && (y - r).abs() < 0.01));
            // 底边恒在 y = h
            assert!(matches!(segs[4], Seg::Line(_, y)
                if (y - h).abs() < 0.01));
        }
    }

    #[test]
    fn retract_keeps_rounded_bottom_corners() {
        // 收回中间帧：底角半径全程 ≥ 上条端部圆角（不会压成直角再弹回圆弧），
        // 且 drop=0 时恰好等于上条端部圆角（与 Widened 形态无缝衔接）
        let (w, bar_h) = (400.0, 32.0);
        let bot_r = BASE_BOT_R * (bar_h / 32.0);
        assert!((card_bottom_radius(w, 0.0, bot_r) - bot_r).abs() < 0.001);
        assert!((card_bottom_radius(w, CARD_DROP_HEIGHT, bot_r) - BOTTOM_RADIUS).abs() < 0.001);
        let mut drop = 1.0;
        while drop <= CARD_DROP_HEIGHT {
            let br = card_bottom_radius(w, drop, bot_r);
            assert!(
                br >= bot_r.min(BOTTOM_RADIUS) - 0.001,
                "drop={drop} 底角过锐: br={br} < {}",
                bot_r.min(BOTTOM_RADIUS)
            );
            let segs = outline(EndStyle::Notch, w, bar_h + drop, bar_h);
            assert_finite_and_in_bounds(&segs, w, bar_h + drop);
            drop += 5.1;
        }
    }

    #[test]
    fn dropped_shape_softens_top_shoulders() {
        let (w, bar_h) = (376.0, 32.0);
        let compact = outline(EndStyle::Notch, w, bar_h, bar_h);
        let expanded = outline(EndStyle::Notch, w, bar_h + CARD_DROP_HEIGHT, bar_h);
        assert!(matches!(compact[0], Seg::Cubic(_, _, _, _, x, y)
            if (x - BASE_TOP_R).abs() < 0.01 && (y - BASE_TOP_R).abs() < 0.01));
        assert!(matches!(expanded[0], Seg::Cubic(_, _, _, _, x, y)
            if (x - EXPANDED_TOP_RADIUS).abs() < 0.01
                && (y - EXPANDED_TOP_RADIUS).abs() < 0.01));
    }

    #[test]
    fn partial_drop_animation_is_well_formed() {
        // 下拉动画中间帧：drop 从 1pt 到全高，轮廓都必须合法且同宽
        let (w, bar_h) = (376.0, 32.0);
        let mut drop = 1.0;
        while drop <= CARD_DROP_HEIGHT {
            let h = bar_h + drop;
            let segs = outline(EndStyle::Notch, w, h, bar_h);
            assert_finite_and_in_bounds(&segs, w, h);
            assert_eq!(segs.len(), 8);
            drop += 3.7;
        }
    }

    #[test]
    fn width_shrink_animation_is_well_formed() {
        // 横缩动画中间帧：宽度从全宽到物理刘海宽，两种形态都必须合法
        let bar_h = 32.0;
        let mut w = 185.0;
        while w <= 376.0 {
            let segs = outline(EndStyle::Notch, w, bar_h, bar_h);
            assert_finite_and_in_bounds(&segs, w, bar_h);
            let h = bar_h + CARD_DROP_HEIGHT;
            let segs = outline(EndStyle::Notch, w, h, bar_h);
            assert_finite_and_in_bounds(&segs, w, h);
            w += 17.3;
        }
    }
}
