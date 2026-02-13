/**
 * WouoUI Port Layer for K9-Pad E73
 *
 * Implements the interface between WouoUI and Rust display driver.
 */

#include "WouoUI_port.h"
#include "WouoUI.h"

// External init function from WouoUI_k9pad.c
extern void WouoUI_UserInit(void);

//--------Minimal string functions for bare-metal--------
#ifdef WOUOUI_EMBEDDED

int wououi_strlen(const char* s) {
    int len = 0;
    while (*s++) len++;
    return len;
}

char* wououi_strcpy(char* dst, const char* src) {
    char* ret = dst;
    while ((*dst++ = *src++));
    return ret;
}

char* wououi_strchr(const char* s, int c) {
    while (*s) {
        if (*s == (char)c) return (char*)s;
        s++;
    }
    return (c == 0) ? (char*)s : (void*)0;
}

int wououi_strcmp(const char* s1, const char* s2) {
    while (*s1 && (*s1 == *s2)) {
        s1++;
        s2++;
    }
    return *(unsigned char*)s1 - *(unsigned char*)s2;
}

void* wououi_memset(void* s, int c, size_t n) {
    unsigned char* p = (unsigned char*)s;
    while (n--) {
        *p++ = (unsigned char)c;
    }
    return s;
}

void* wououi_memcpy(void* dst, const void* src, size_t n) {
    unsigned char* d = (unsigned char*)dst;
    const unsigned char* s = (const unsigned char*)src;
    while (n--) {
        *d++ = *s++;
    }
    return dst;
}

// Simple integer to string conversion (only supports "%d")
int wououi_sprintf(char* str, const char* format, ...) {
    if (format[0] == '%' && format[1] == 'd' && format[2] == '\0') {
        __builtin_va_list ap;
        __builtin_va_start(ap, format);
        int num = __builtin_va_arg(ap, int);
        __builtin_va_end(ap);

        char* p = str;
        int negative = 0;

        if (num < 0) {
            negative = 1;
            num = -num;
        }

        // Convert to string (reversed)
        char temp[12];
        int i = 0;
        do {
            temp[i++] = '0' + (num % 10);
            num /= 10;
        } while (num > 0);

        if (negative) {
            *p++ = '-';
        }

        // Reverse the digits
        while (i > 0) {
            *p++ = temp[--i];
        }
        *p = '\0';

        return p - str;
    }
    return 0;
}

#endif

// Dirty flag - set when buffer updated
volatile uint8_t g_screen_dirty = 0;

// Menu active flag
volatile uint8_t g_menu_active = 0;

// Send function - sets dirty flag for Rust to read buffer
static void port_send_buff(ScreenBuff buff) {
    (void)buff;
    g_screen_dirty = 1;
}

// Initialize WouoUI
void WouoUI_PortInit(void) {
    // Use default UI instance
    WouoUI_SelectDefaultUI();

    // Attach our send function
    WouoUI_AttachSendBuffFun(port_send_buff);

    // Initialize user-defined pages (K9-Pad menus)
    WouoUI_UserInit();

    g_menu_active = 0;
    g_screen_dirty = 0;
}

// Process one frame - call this at desired FPS
// elapsed_ms: time since last call in milliseconds
uint8_t WouoUI_PortTick(uint16_t elapsed_ms) {
    // Update UI state machine
    WouoUI_Proc(elapsed_ms);

    // Return dirty flag and clear it
    uint8_t was_dirty = g_screen_dirty;
    g_screen_dirty = 0;
    return was_dirty;
}

// Send input to WouoUI
void WouoUI_PortSendInput(uint8_t input_type) {
    InputMsg msg = msg_none;

    switch (input_type) {
        case INPUT_UP:
            msg = msg_up;
            break;
        case INPUT_DOWN:
            msg = msg_down;
            break;
        case INPUT_LEFT:
            msg = msg_left;
            break;
        case INPUT_RIGHT:
            msg = msg_right;
            break;
        case INPUT_CLICK:
            msg = msg_click;
            break;
        case INPUT_RETURN:
            msg = msg_return;
            break;
        default:
            return;
    }

    WOUOUI_MSG_QUE_SEND(msg);
}

// Check if menu is active
uint8_t WouoUI_PortIsMenuActive(void) {
    return g_menu_active;
}

// Enter menu mode
void WouoUI_PortEnterMenu(void) {
    g_menu_active = 1;
}

// Exit menu mode
void WouoUI_PortExitMenu(void) {
    g_menu_active = 0;
    // 退出时清空消息队列，防止残留消息影响下次进入
    WOUOUI_MSG_QUE_CLEAR();
}

// Reset WouoUI to clean entry state (call before entering menu)
void WouoUI_PortResetForEntry(void) {
    // 清空消息队列
    WOUOUI_MSG_QUE_CLEAR();

    // 重置到主页
    p_cur_ui->current_page = p_cur_ui->home_page;
    p_cur_ui->in_page = p_cur_ui->home_page;
    p_cur_ui->state = ui_page_in;

    // 重置模糊状态（入场不需要模糊）
    p_cur_ui->ui_blur.blur_cur = 0;
    p_cur_ui->ui_blur.blur_tgt = 0;
    p_cur_ui->ui_blur.blur_end = true;
    p_cur_ui->ui_blur.timer = 0;

    // 指示器直接从目标位置开始（不从全屏开始，避免反色导致全屏闪白）
    int16_t ind_x = WOUOUI_MIDDLE_H - TILE_ICON_W/2 - TILE_ICON_IND_L;
    int16_t ind_y = TILE_ICON_U - TILE_ICON_IND_U;
    p_cur_ui->indicator.x = (AnimPos){ind_x, ind_x, 0};
    p_cur_ui->indicator.y = (AnimPos){ind_y, ind_y, 0};
    p_cur_ui->indicator.w = (AnimPos){TILE_ICON_IND_W, TILE_ICON_IND_W, 0};
    p_cur_ui->indicator.h = (AnimPos){TILE_ICON_IND_H, TILE_ICON_IND_H, 0};

    // 重置滚动条
    p_cur_ui->scrollBar.y = (AnimPos){0, 0, 0};

    // 初始化入场动画参数
    ((Page*)p_cur_ui->home_page)->methods->in_para_init(p_cur_ui->home_page);

    // 确保状态机开始处理
#if SOFTWARE_DYNAMIC_REFRESH
    p_cur_ui->is_motionless = false;
#endif
    p_cur_ui->anim_is_finish = false;
    p_cur_ui->slide_is_finish = false;
}

// Get buffer pointer - returns pointer to the screen buffer
uint8_t* WouoUI_PortGetBuffer(void) {
    return (uint8_t*)&(p_cur_ui->screen_buff);
}

// Get buffer size
uint16_t WouoUI_PortGetBufferSize(void) {
    return SCREEN_BUFF_SIZE;
}

// Check if currently on the home page
uint8_t WouoUI_PortIsOnHomePage(void) {
    return (p_cur_ui->current_page == p_cur_ui->home_page) ? 1 : 0;
}

// Configure animation parameters for target frame time
// Call after PortInit to adapt blur timing for the actual FPS
void WouoUI_PortConfigFrameTime(uint16_t frame_ms) {
    if (frame_ms == 0) frame_ms = 20;

    // FADE_ANI controls blur step interval for page-out fade (0→4).
    // Page-in fade is skipped for non-window transitions (expansion = reveal).
    //
    // Target ~20ms per blur step (80-96ms total) for snappy page-out.
    // Formula: FADE_ANI = (ceil(20/frame_ms) - 2) * frame_ms
    // At 20ms frame: (1→3min -2)*20 = 20  (original value)
    // At  8ms frame: (3-2)*8  = 8          (96ms total, fast)
    uint16_t target_frames_per_step = (20 + frame_ms - 1) / frame_ms; // ceil(20/frame_ms)
    if (target_frames_per_step < 3) target_frames_per_step = 3; // minimum 1 acc + 1 check + 1 step
    uint16_t fade_ani = (target_frames_per_step - 2) * frame_ms;
    if (fade_ani == 0) fade_ani = 1;

    p_cur_ui->upara->ani_param[FADE_ANI] = fade_ani;
}
