// menu/animation.rs - 动画系统
//
// 参考 WouoUI 的动画设计：
// - 非线性缓动函数
// - 平滑滚动
// - 选中指示器动画

/// 缓动函数类型
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EasingType {
    /// 线性
    Linear,
    /// 缓入
    EaseIn,
    /// 缓出
    EaseOut,
    /// 缓入缓出
    EaseInOut,
}

/// 动画状态
#[derive(Clone, Copy, Debug)]
pub struct Animation {
    /// 起始值
    start: i16,
    /// 目标值
    target: i16,
    /// 当前值
    current: i16,
    /// 动画持续帧数
    duration: u8,
    /// 当前帧
    frame: u8,
    /// 缓动类型
    easing: EasingType,
    /// 是否正在运行
    running: bool,
}

impl Animation {
    pub const fn new() -> Self {
        Self {
            start: 0,
            target: 0,
            current: 0,
            duration: 10,
            frame: 0,
            easing: EasingType::EaseOut,
            running: false,
        }
    }

    /// 开始动画
    pub fn start(&mut self, from: i16, to: i16, duration: u8, easing: EasingType) {
        self.start = from;
        self.target = to;
        self.current = from;
        self.duration = duration.max(1);
        self.frame = 0;
        self.easing = easing;
        self.running = true;
    }

    /// 更新动画，返回当前值
    pub fn update(&mut self) -> i16 {
        if !self.running {
            return self.current;
        }

        self.frame += 1;

        if self.frame >= self.duration {
            self.current = self.target;
            self.running = false;
        } else {
            let t = self.frame as f32 / self.duration as f32;
            let eased_t = self.apply_easing(t);
            let delta = self.target - self.start;
            self.current = self.start + (delta as f32 * eased_t) as i16;
        }

        self.current
    }

    /// 应用缓动函数
    fn apply_easing(&self, t: f32) -> f32 {
        match self.easing {
            EasingType::Linear => t,
            EasingType::EaseIn => t * t,
            EasingType::EaseOut => 1.0 - (1.0 - t) * (1.0 - t),
            EasingType::EaseInOut => {
                if t < 0.5 {
                    2.0 * t * t
                } else {
                    let x = -2.0 * t + 2.0;
                    1.0 - (x * x) / 2.0
                }
            }
        }
    }

    /// 是否正在运行
    pub fn is_running(&self) -> bool {
        self.running
    }

    /// 获取当前值
    pub fn value(&self) -> i16 {
        self.current
    }

    /// 设置目标值（立即跳转）
    pub fn set_immediate(&mut self, value: i16) {
        self.start = value;
        self.target = value;
        self.current = value;
        self.running = false;
    }
}

/// 滚动动画管理器
#[derive(Clone, Copy)]
pub struct ScrollAnimator {
    /// Y 轴滚动动画
    pub scroll_y: Animation,
    /// 选中指示器 X 位置动画
    pub indicator_x: Animation,
    /// 选中指示器宽度动画
    pub indicator_width: Animation,
}

impl ScrollAnimator {
    pub const fn new() -> Self {
        Self {
            scroll_y: Animation::new(),
            indicator_x: Animation::new(),
            indicator_width: Animation::new(),
        }
    }

    /// 更新所有动画
    pub fn update(&mut self) {
        self.scroll_y.update();
        self.indicator_x.update();
        self.indicator_width.update();
    }

    /// 检查是否有动画正在运行
    pub fn is_animating(&self) -> bool {
        self.scroll_y.is_running()
            || self.indicator_x.is_running()
            || self.indicator_width.is_running()
    }

    /// 开始滚动动画
    pub fn scroll_to(&mut self, target_y: i16, duration: u8) {
        let current = self.scroll_y.value();
        if current != target_y {
            self.scroll_y.start(current, target_y, duration, EasingType::EaseOut);
        }
    }

    /// 开始指示器动画
    pub fn move_indicator(&mut self, target_x: i16, target_width: i16, duration: u8) {
        let current_x = self.indicator_x.value();
        let current_width = self.indicator_width.value();

        if current_x != target_x {
            self.indicator_x.start(current_x, target_x, duration, EasingType::EaseOut);
        }
        if current_width != target_width {
            self.indicator_width
                .start(current_width, target_width, duration, EasingType::EaseOut);
        }
    }
}

/// 简单的线性插值（不使用浮点数版本，适用于资源受限环境）
pub fn lerp_i16(a: i16, b: i16, t_256: u8) -> i16 {
    // t_256: 0-255 表示 0.0-1.0
    let delta = (b as i32) - (a as i32);
    let result = (a as i32) + (delta * t_256 as i32 / 256);
    result as i16
}

/// 简单的缓出函数（整数版本）
/// t_256: 输入 0-255
/// 返回: 0-255
pub fn ease_out_quad_i16(t_256: u8) -> u8 {
    // ease_out = 1 - (1 - t)^2
    // 使用整数运算：result = 255 - (255 - t)^2 / 255
    let inv_t = 255u16 - t_256 as u16;
    let sq = inv_t * inv_t / 255;
    (255 - sq) as u8
}

/// 简单的缓入缓出函数（整数版本）
pub fn ease_in_out_quad_i16(t_256: u8) -> u8 {
    if t_256 < 128 {
        // ease_in 部分
        let t2 = (t_256 as u16) * 2;
        (t2 * t2 / 512) as u8
    } else {
        // ease_out 部分
        let t2 = (t_256 as u16 - 128) * 2;
        let inv_t = 255 - t2;
        (128 + (127 - inv_t * inv_t / 510)) as u8
    }
}

impl Default for Animation {
    fn default() -> Self {
        Self::new()
    }
}

impl Default for ScrollAnimator {
    fn default() -> Self {
        Self::new()
    }
}

// ============== 单元测试 ==============

#[cfg(test)]
mod tests {
    use super::*;

    // -------- EasingType 测试 --------

    #[test]
    fn test_easing_type_equality() {
        assert_eq!(EasingType::Linear, EasingType::Linear);
        assert_ne!(EasingType::Linear, EasingType::EaseIn);
    }

    // -------- Animation 测试 --------

    #[test]
    fn test_animation_new() {
        let anim = Animation::new();
        assert_eq!(anim.value(), 0);
        assert!(!anim.is_running());
    }

    #[test]
    fn test_animation_start() {
        let mut anim = Animation::new();
        anim.start(0, 100, 10, EasingType::Linear);

        assert!(anim.is_running());
        assert_eq!(anim.value(), 0); // 起始值
    }

    #[test]
    fn test_animation_linear_progress() {
        let mut anim = Animation::new();
        anim.start(0, 100, 10, EasingType::Linear);

        // 更新 5 帧（50%）
        for _ in 0..5 {
            anim.update();
        }

        // 线性插值，应该接近 50
        let value = anim.value();
        assert!(value >= 45 && value <= 55, "Value {} should be around 50", value);
        assert!(anim.is_running());
    }

    #[test]
    fn test_animation_completes() {
        let mut anim = Animation::new();
        anim.start(0, 100, 10, EasingType::Linear);

        // 更新到完成
        for _ in 0..15 {
            anim.update();
        }

        assert_eq!(anim.value(), 100); // 到达目标值
        assert!(!anim.is_running()); // 停止运行
    }

    #[test]
    fn test_animation_negative_values() {
        let mut anim = Animation::new();
        anim.start(50, -50, 10, EasingType::Linear);

        // 完成动画
        for _ in 0..15 {
            anim.update();
        }

        assert_eq!(anim.value(), -50);
    }

    #[test]
    fn test_animation_set_immediate() {
        let mut anim = Animation::new();
        anim.start(0, 100, 10, EasingType::Linear);

        // 立即设置值
        anim.set_immediate(75);

        assert_eq!(anim.value(), 75);
        assert!(!anim.is_running());
    }

    #[test]
    fn test_animation_ease_out() {
        let mut anim = Animation::new();
        anim.start(0, 100, 10, EasingType::EaseOut);

        // 更新 3 帧（30%）
        for _ in 0..3 {
            anim.update();
        }

        // EaseOut 在开始时移动更快，所以值应该大于线性的 30
        let value = anim.value();
        assert!(value > 30, "EaseOut value {} should be > 30 at 30% progress", value);
    }

    #[test]
    fn test_animation_ease_in() {
        let mut anim = Animation::new();
        anim.start(0, 100, 10, EasingType::EaseIn);

        // 更新 3 帧（30%）
        for _ in 0..3 {
            anim.update();
        }

        // EaseIn 在开始时移动更慢，所以值应该小于线性的 30
        let value = anim.value();
        assert!(value < 30, "EaseIn value {} should be < 30 at 30% progress", value);
    }

    #[test]
    fn test_animation_update_when_not_running() {
        let mut anim = Animation::new();
        anim.set_immediate(50);

        // 更新不运行的动画应该返回当前值
        let value = anim.update();
        assert_eq!(value, 50);
    }

    #[test]
    fn test_animation_minimum_duration() {
        let mut anim = Animation::new();
        // 尝试 0 持续时间，应该被强制为 1
        anim.start(0, 100, 0, EasingType::Linear);

        anim.update();
        assert_eq!(anim.value(), 100); // 1 帧后完成
        assert!(!anim.is_running());
    }

    // -------- ScrollAnimator 测试 --------

    #[test]
    fn test_scroll_animator_new() {
        let animator = ScrollAnimator::new();
        assert!(!animator.is_animating());
    }

    #[test]
    fn test_scroll_animator_scroll_to() {
        let mut animator = ScrollAnimator::new();
        animator.scroll_to(100, 10);

        assert!(animator.is_animating());
        assert!(animator.scroll_y.is_running());
    }

    #[test]
    fn test_scroll_animator_scroll_same_position() {
        let mut animator = ScrollAnimator::new();
        animator.scroll_y.set_immediate(50);

        // 滚动到相同位置不应该启动动画
        animator.scroll_to(50, 10);
        assert!(!animator.scroll_y.is_running());
    }

    #[test]
    fn test_scroll_animator_move_indicator() {
        let mut animator = ScrollAnimator::new();
        animator.move_indicator(20, 80, 10);

        assert!(animator.indicator_x.is_running());
        assert!(animator.indicator_width.is_running());
    }

    #[test]
    fn test_scroll_animator_update() {
        let mut animator = ScrollAnimator::new();
        animator.scroll_to(100, 5);

        // 更新所有动画
        for _ in 0..10 {
            animator.update();
        }

        assert_eq!(animator.scroll_y.value(), 100);
        assert!(!animator.is_animating());
    }

    #[test]
    fn test_scroll_animator_partial_indicator_update() {
        let mut animator = ScrollAnimator::new();
        animator.indicator_x.set_immediate(10);
        animator.indicator_width.set_immediate(50);

        // 只更新 X，宽度相同
        animator.move_indicator(20, 50, 10);

        assert!(animator.indicator_x.is_running());
        assert!(!animator.indicator_width.is_running());
    }

    // -------- 辅助函数测试 --------

    #[test]
    fn test_lerp_i16_start() {
        // t=0 应该返回 a
        assert_eq!(lerp_i16(0, 100, 0), 0);
    }

    #[test]
    fn test_lerp_i16_end() {
        // t=255 应该接近 b
        let result = lerp_i16(0, 100, 255);
        assert!(result >= 99, "lerp result {} should be ~100", result);
    }

    #[test]
    fn test_lerp_i16_middle() {
        // t=128 应该接近中点
        let result = lerp_i16(0, 100, 128);
        assert!(result >= 48 && result <= 52, "lerp result {} should be ~50", result);
    }

    #[test]
    fn test_lerp_i16_negative() {
        let result = lerp_i16(-50, 50, 128);
        assert!(result >= -2 && result <= 2, "lerp result {} should be ~0", result);
    }

    #[test]
    fn test_ease_out_quad_i16_start() {
        assert_eq!(ease_out_quad_i16(0), 0);
    }

    #[test]
    fn test_ease_out_quad_i16_end() {
        assert_eq!(ease_out_quad_i16(255), 255);
    }

    #[test]
    fn test_ease_out_quad_i16_faster_start() {
        // EaseOut 在前半段应该移动更快
        let mid = ease_out_quad_i16(128);
        assert!(mid > 128, "EaseOut at 50% ({}) should be > 128", mid);
    }

    #[test]
    fn test_ease_in_out_quad_i16_start() {
        assert_eq!(ease_in_out_quad_i16(0), 0);
    }

    #[test]
    fn test_ease_in_out_quad_i16_middle() {
        let mid = ease_in_out_quad_i16(128);
        // 在中点附近应该接近 128
        assert!(mid >= 120 && mid <= 136, "EaseInOut at 50% ({}) should be ~128", mid);
    }

    // -------- 场景测试 --------

    #[test]
    fn test_menu_scroll_animation() {
        // 模拟菜单滚动场景
        let mut animator = ScrollAnimator::new();

        // 用户向下滚动
        animator.scroll_to(20, 8);

        // 模拟帧更新
        let mut frames = 0;
        while animator.is_animating() && frames < 20 {
            animator.update();
            frames += 1;
        }

        assert_eq!(animator.scroll_y.value(), 20);
        assert!(frames <= 10, "Animation should complete in ~8 frames");
    }

    #[test]
    fn test_indicator_transition() {
        // 模拟选中指示器从一个菜单项移动到另一个
        let mut animator = ScrollAnimator::new();
        animator.indicator_x.set_immediate(0);
        animator.indicator_width.set_immediate(60);

        // 移动到新位置
        animator.move_indicator(0, 80, 6);

        // 完成动画
        for _ in 0..10 {
            animator.update();
        }

        assert_eq!(animator.indicator_width.value(), 80);
    }
}
