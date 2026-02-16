// INPUT:  WouoUI_anim.h
// OUTPUT: WouoUI_Animation(), WouoUI_SlideAnimation(), WouoUI_AnimIsEnd()
// POS:    非线性插值动画引擎，驱动所有 UI 过渡效果
#include "WouoUI_anim.h"

/**
 * @brief 非线性运动函数
 *
 * @param animStr[in/out] 动画结构体
 * @param aniTime[in] 动画时间参数
 * @param inrtime[in] 轮序间隔时间
 * @param ret[out] 动画是否结束的结果指针(用于统计所有动画是否结束,true表示结束)
 */
void WouoUI_Animation(AnimPos *animStr, uint16_t aniTime, uint16_t inrTime, uint8_t* ret) {
    uint8_t temp = false; //默认动画没有结束
    if (animStr->pos_cur != animStr->pos_tgt) {
        uint16_t divisor = (inrTime > 0) ? (aniTime / inrTime) : 0;
        if (divisor == 0) {
            // 帧时间超过动画时间，直接到达目标位置
            animStr->pos_cur = animStr->pos_tgt;
            animStr->pos_err = 0;
        } else {
            animStr->pos_err += (animStr->pos_tgt - animStr->pos_cur);
            animStr->pos_cur += animStr->pos_err / divisor;
            animStr->pos_err %= divisor;
        }
    } else {
        animStr->pos_err = 0;
        temp = true;
    }
    (*ret) = temp && (*ret);
}

