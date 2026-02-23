// INPUT:  WouoUI.h
// OUTPUT: K9Pad_MenuInit() — 8 主菜单项 + Layer/User/Settings/About 子页面 + SetBrightness + ScreenTimeout + QuickMenu
// POS:    K9-Pad 专用菜单树定义，被 WouoUI_port.c 调用
/**
 * WouoUI K9-Pad Menu Configuration
 *
 * TitlePage main menu (8 items):
 *   Layer 0 / Layer 1 / Layer 2 / Layer 3 / Layer 4 / User / Settings / About
 *
 * Sub-pages:
 *   Layer 0/1/2/3/4: "Enable" action + Data Ch / Volume / Subs / Time checkboxes
 *   User: User A/B/C radio (BLE multi-device) + Clear Bond
 *   Settings: BLE toggle + Brightness slider + Screen Off selector + DFU Mode + To Bootloader
 *   About: firmware info
 */

#include "WouoUI.h"

//--------定义页面对象
static TitlePage main_page;
static ListPage pad_a_page;
static ListPage pad_b_page;
static ListPage pad_c_page;
static ListPage pad_d_page;
static ListPage pad_e_page;
static ListPage user_page;
static ListPage settings_page;
static ListPage about_page;
static MsgWin msg_win;
static ConfWin dfu_conf_win;
static ValWin brightness_win;
static ListWin screen_timeout_win;

//--------DFU/Bootloader 确认弹窗的 pending action (1=DFU, 2=Bootloader)
static uint8_t pending_dfu_action = 0;

//--------当前选中的 Layer (0-4)
static uint8_t g_selected_pad = 0;

//--------菜单退出请求标志 (由回调设置，Rust 侧轮询)
static uint8_t g_exit_requested = 0;

//--------DFU 模式请求标志 (由 Settings 回调设置，Rust 侧轮询)
static uint8_t g_dfu_requested = 0;

//--------USB Bootloader 请求标志 (由 Settings 回调设置，Rust 侧轮询)
static uint8_t g_usb_bl_requested = 0;

//--------Screen timeout 选项 (ListWin 选择器)
static char* screen_timeout_options[5] = {
    (char*)"5s", (char*)"10s", (char*)"20s", (char*)"30s", (char*)"1min"
};


//--------页面选项数量
#define MAIN_PAGE_NUM       8
#define PAD_PAGE_NUM        5
#define USER_PAGE_NUM       5
#define SETTINGS_PAGE_NUM   7
#define ABOUT_PAGE_NUM      4

//--------主菜单选项
static Option main_option_array[MAIN_PAGE_NUM] = {
    {.text = (char *)"+ Layer 0"},
    {.text = (char *)"+ Layer 1"},
    {.text = (char *)"+ Layer 2"},
    {.text = (char *)"+ Layer 3"},
    {.text = (char *)"+ Layer 4"},
    {.text = (char *)"+ User"},
    {.text = (char *)"+ Settings"},
    {.text = (char *)"+ About"}
};

// 主菜单图标 (30x30) - 8 icons
// Layer 0/1/2/3/4: digit 0/1/2/3/4, User: Home icon, Settings: BLE icon, About: Info icon
static Icon main_icon_array[MAIN_PAGE_NUM] = {
    // [0] Layer 0 - Rounded rect + digit "0"
    [0] = {
        0x00, 0x00, 0xF8, 0xFC, 0xFE, 0x0E, 0x06, 0x06, 0x06, 0x06, 0x06, 0x06, 0x06, 0x06, 0x06, 0x06, 0x06, 0x06, 0x06, 0x06, 0x06, 0x06, 0x06, 0x0E, 0xFE, 0xFC, 0xF8, 0x00, 0x00, 0x00,
        0x00, 0x00, 0xFF, 0xFF, 0xFF, 0x00, 0x00, 0x00, 0x00, 0xFE, 0xFF, 0x03, 0x03, 0x03, 0x03, 0x03, 0x03, 0xFF, 0xFE, 0x00, 0x00, 0x00, 0x00, 0x00, 0xFF, 0xFF, 0xFF, 0x00, 0x00, 0x00,
        0x00, 0x00, 0xFF, 0xFF, 0xFF, 0x00, 0x00, 0x00, 0x00, 0x7F, 0xFF, 0xC0, 0xC0, 0xC0, 0xC0, 0xC0, 0xC0, 0xFF, 0x7F, 0x00, 0x00, 0x00, 0x00, 0x00, 0xFF, 0xFF, 0xFF, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x1F, 0x3F, 0x7F, 0x70, 0x60, 0x60, 0x60, 0x60, 0x60, 0x60, 0x60, 0x60, 0x60, 0x60, 0x60, 0x60, 0x60, 0x60, 0x60, 0x60, 0x60, 0x70, 0x7F, 0x3F, 0x1F, 0x00, 0x00, 0x00
    },
    // [1] Layer 1 - Rounded rect + digit "1"
    [1] = {
        0x00, 0x00, 0xF8, 0xFC, 0xFE, 0x0E, 0x06, 0x06, 0x06, 0x06, 0x06, 0x06, 0x06, 0x06, 0x06, 0x06, 0x06, 0x06, 0x06, 0x06, 0x06, 0x06, 0x06, 0x0E, 0xFE, 0xFC, 0xF8, 0x00, 0x00, 0x00,
        0x00, 0x00, 0xFF, 0xFF, 0xFF, 0x00, 0x00, 0x00, 0x00, 0x00, 0x08, 0x0C, 0x0E, 0xFF, 0xFF, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0xFF, 0xFF, 0xFF, 0x00, 0x00, 0x00,
        0x00, 0x00, 0xFF, 0xFF, 0xFF, 0x00, 0x00, 0x00, 0x00, 0x00, 0xC0, 0xC0, 0xC0, 0xFF, 0xFF, 0xC0, 0xC0, 0xC0, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0xFF, 0xFF, 0xFF, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x1F, 0x3F, 0x7F, 0x70, 0x60, 0x60, 0x60, 0x60, 0x60, 0x60, 0x60, 0x60, 0x60, 0x60, 0x60, 0x60, 0x60, 0x60, 0x60, 0x60, 0x60, 0x70, 0x7F, 0x3F, 0x1F, 0x00, 0x00, 0x00
    },
    // [2] Layer 2 - Rounded rect + digit "2"
    [2] = {
        0x00, 0x00, 0xF8, 0xFC, 0xFE, 0x0E, 0x06, 0x06, 0x06, 0x06, 0x06, 0x06, 0x06, 0x06, 0x06, 0x06, 0x06, 0x06, 0x06, 0x06, 0x06, 0x06, 0x06, 0x0E, 0xFE, 0xFC, 0xF8, 0x00, 0x00, 0x00,
        0x00, 0x00, 0xFF, 0xFF, 0xFF, 0x00, 0x00, 0x00, 0x00, 0x06, 0x07, 0x03, 0x83, 0x83, 0xC3, 0xC3, 0xE3, 0x7F, 0x3E, 0x00, 0x00, 0x00, 0x00, 0x00, 0xFF, 0xFF, 0xFF, 0x00, 0x00, 0x00,
        0x00, 0x00, 0xFF, 0xFF, 0xFF, 0x00, 0x00, 0x00, 0x00, 0xFE, 0xFF, 0xC3, 0xC3, 0xC1, 0xC1, 0xC0, 0xC0, 0xC0, 0xC0, 0x00, 0x00, 0x00, 0x00, 0x00, 0xFF, 0xFF, 0xFF, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x1F, 0x3F, 0x7F, 0x70, 0x60, 0x60, 0x60, 0x60, 0x60, 0x60, 0x60, 0x60, 0x60, 0x60, 0x60, 0x60, 0x60, 0x60, 0x60, 0x60, 0x60, 0x70, 0x7F, 0x3F, 0x1F, 0x00, 0x00, 0x00
    },
    // [3] Layer 3 - Rounded rect + digit "3"
    [3] = {
        0x00, 0x00, 0xF8, 0xFC, 0xFE, 0x0E, 0x06, 0x06, 0x06, 0x06, 0x06, 0x06, 0x06, 0x06, 0x06, 0x06, 0x06, 0x06, 0x06, 0x06, 0x06, 0x06, 0x06, 0x0E, 0xFE, 0xFC, 0xF8, 0x00, 0x00, 0x00,
        0x00, 0x00, 0xFF, 0xFF, 0xFF, 0x00, 0x00, 0x00, 0x00, 0x06, 0x07, 0x03, 0x03, 0xC3, 0xC3, 0xC3, 0xC3, 0xFF, 0x3E, 0x00, 0x00, 0x00, 0x00, 0x00, 0xFF, 0xFF, 0xFF, 0x00, 0x00, 0x00,
        0x00, 0x00, 0xFF, 0xFF, 0xFF, 0x00, 0x00, 0x00, 0x00, 0x60, 0xE0, 0xC0, 0xC0, 0xC0, 0xC0, 0xC0, 0xC0, 0xFF, 0x7F, 0x00, 0x00, 0x00, 0x00, 0x00, 0xFF, 0xFF, 0xFF, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x1F, 0x3F, 0x7F, 0x70, 0x60, 0x60, 0x60, 0x60, 0x60, 0x60, 0x60, 0x60, 0x60, 0x60, 0x60, 0x60, 0x60, 0x60, 0x60, 0x60, 0x60, 0x70, 0x7F, 0x3F, 0x1F, 0x00, 0x00, 0x00
    },
    // [4] Layer 4 - Rounded rect + digit "4"
    [4] = {
        0x00, 0x00, 0xF8, 0xFC, 0xFE, 0x0E, 0x06, 0x06, 0x06, 0x06, 0x06, 0x06, 0x06, 0x06, 0x06, 0x06, 0x06, 0x06, 0x06, 0x06, 0x06, 0x06, 0x06, 0x0E, 0xFE, 0xFC, 0xF8, 0x00, 0x00, 0x00,
        0x00, 0x00, 0xFF, 0xFF, 0xFF, 0x00, 0x00, 0x00, 0xC0, 0xE0, 0x30, 0x18, 0x0C, 0xFE, 0xFF, 0xFF, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0xFF, 0xFF, 0xFF, 0x00, 0x00, 0x00,
        0x00, 0x00, 0xFF, 0xFF, 0xFF, 0x00, 0x00, 0x00, 0x03, 0x03, 0x03, 0x03, 0x03, 0xFF, 0xFF, 0xFF, 0x03, 0x03, 0x03, 0x00, 0x00, 0x00, 0x00, 0x00, 0xFF, 0xFF, 0xFF, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x1F, 0x3F, 0x7F, 0x70, 0x60, 0x60, 0x60, 0x60, 0x60, 0x60, 0x60, 0x60, 0x60, 0x60, 0x60, 0x60, 0x60, 0x60, 0x60, 0x60, 0x60, 0x70, 0x7F, 0x3F, 0x1F, 0x00, 0x00, 0x00
    },
    // [5] User - Home icon (person's home)
    [5] = {
        0x00, 0x00, 0x00, 0x00, 0x00, 0x80, 0xC0, 0xE0, 0xF0, 0xF8, 0xFC, 0xFE, 0xFF, 0xFE, 0xFC,
        0xFC, 0xFE, 0xFF, 0xFE, 0xFC, 0xF8, 0xF0, 0xE0, 0xC0, 0x80, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x00, 0xF0, 0xFE, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF,
        0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFE, 0xF0, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x0F, 0x7F, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0x0F, 0x0F, 0x0F, 0x0F, 0x0F,
        0x0F, 0x0F, 0x0F, 0x0F, 0x0F, 0xFF, 0xFF, 0xFF, 0xFF, 0x7F, 0x0F, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x00, 0x00, 0x01, 0x03, 0x07, 0x0F, 0x1F, 0x1C, 0x1C, 0x1C, 0x1C, 0x1C,
        0x1C, 0x1C, 0x1C, 0x1C, 0x1C, 0x1F, 0x0F, 0x07, 0x03, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00
    },
    // [6] Settings - Gear icon
    [6] = {
        0x00, 0x00, 0x00, 0x00, 0x80, 0xE0, 0xC0, 0x80, 0x00, 0x00, 0x80, 0x8C, 0xFC, 0xFC, 0xFC, 0xFC, 0xFC, 0xFC, 0x8C, 0x80, 0x00, 0x00, 0x80, 0xC0, 0xE0, 0x80, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x0C, 0x1F, 0x1F, 0x1F, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0x3F, 0x0F, 0x0F, 0x07, 0x07, 0x0F, 0x0F, 0x3F, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0x1F, 0x1F, 0x1F, 0x0C, 0x00, 0x00,
        0x00, 0x00, 0x0C, 0x3E, 0x7E, 0xFE, 0xFF, 0x7F, 0x3F, 0x3F, 0x7F, 0x7F, 0xFC, 0xFC, 0xF8, 0xF8, 0xFC, 0xFC, 0x7F, 0x7F, 0x3F, 0x3F, 0x7F, 0xFF, 0xFE, 0x7E, 0x3E, 0x0C, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x0C, 0x0F, 0x0F, 0x0F, 0x0F, 0x0F, 0x0F, 0x0C, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00
    },
    // [7] About - Info icon
    [7] = {
        0x00, 0x00, 0x80, 0xE0, 0xF0, 0xF8, 0xFC, 0x3C, 0x1E, 0x0E, 0x0E, 0x06, 0x06, 0x06, 0x06,
        0x06, 0x06, 0x06, 0x06, 0x0E, 0x0E, 0x1E, 0x3C, 0xFC, 0xF8, 0xF0, 0xE0, 0x80, 0x00, 0x00,
        0x00, 0xFE, 0xFF, 0xFF, 0xFF, 0xFF, 0x01, 0x00, 0x00, 0x00, 0xFC, 0xFC, 0xFC, 0xFC, 0x00,
        0x00, 0xFC, 0xFC, 0xFC, 0xFC, 0x00, 0x00, 0x00, 0x01, 0xFF, 0xFF, 0xFF, 0xFF, 0xFE, 0x00,
        0x00, 0x7F, 0xFF, 0xFF, 0xFF, 0xFF, 0x80, 0x00, 0x00, 0x00, 0x1F, 0x1F, 0x1F, 0x1F, 0x00,
        0x00, 0x1F, 0x1F, 0x1F, 0x1F, 0x00, 0x00, 0x00, 0x80, 0xFF, 0xFF, 0xFF, 0xFF, 0x7F, 0x00,
        0x00, 0x00, 0x01, 0x07, 0x0F, 0x1F, 0x3F, 0x3C, 0x78, 0x70, 0x70, 0x60, 0x60, 0x60, 0x60,
        0x60, 0x60, 0x60, 0x60, 0x70, 0x70, 0x78, 0x3C, 0x3F, 0x1F, 0x0F, 0x07, 0x01, 0x00, 0x00
    }
};

//--------Layer 0 子页面选项
static Option pad_a_option_array[PAD_PAGE_NUM] = {
    {.text = (char *)"- Layer 0"},
    {.text = (char *)"! Enable"},
    {.text = (char *)"@ Volume",  .val = 0},
    {.text = (char *)"@ Subs",    .val = 0},
    {.text = (char *)"@ Time",    .val = 0}
};

//--------Layer 1 子页面选项
static Option pad_b_option_array[PAD_PAGE_NUM] = {
    {.text = (char *)"- Layer 1"},
    {.text = (char *)"! Enable"},
    {.text = (char *)"@ Volume",  .val = 0},
    {.text = (char *)"@ Subs",    .val = 0},
    {.text = (char *)"@ Time",    .val = 0}
};

//--------Layer 2 子页面选项
static Option pad_c_option_array[PAD_PAGE_NUM] = {
    {.text = (char *)"- Layer 2"},
    {.text = (char *)"! Enable"},
    {.text = (char *)"@ Volume",  .val = 0},
    {.text = (char *)"@ Subs",    .val = 0},
    {.text = (char *)"@ Time",    .val = 0}
};

//--------Layer 3 子页面选项
static Option pad_d_option_array[PAD_PAGE_NUM] = {
    {.text = (char *)"- Layer 3"},
    {.text = (char *)"! Enable"},
    {.text = (char *)"@ Volume",  .val = 0},
    {.text = (char *)"@ Subs",    .val = 0},
    {.text = (char *)"@ Time",    .val = 0}
};

//--------Layer 4 子页面选项
static Option pad_e_option_array[PAD_PAGE_NUM] = {
    {.text = (char *)"- Layer 4"},
    {.text = (char *)"! Enable"},
    {.text = (char *)"@ Volume",  .val = 0},
    {.text = (char *)"@ Subs",    .val = 0},
    {.text = (char *)"@ Time",    .val = 0}
};

//--------User 页面选项 (BLE 多设备)
static Option user_option_array[USER_PAGE_NUM] = {
    {.text = (char *)"- User"},
    {.text = (char *)"@ User A", .val = 1},
    {.text = (char *)"@ User B", .val = 0},
    {.text = (char *)"@ User C", .val = 0},
    {.text = (char *)"# Clear Bond"}
};

//--------Settings 页面选项
static Option settings_option_array[SETTINGS_PAGE_NUM] = {
    {.text = (char *)"- Settings"},
    {.text = (char *)"@ BLE", .val = 1},
    {.text = (char *)"~ Brightness", .val = 80},
    {.text = (char *)"> Screen Off", .content = (char*)"20s"},
    {.text = (char *)"@ Quick Menu", .val = 0},
    {.text = (char *)"! DFU Mode"},
    {.text = (char *)"! Bootloader Mode"}
};

//--------About 页面选项
static Option about_option_array[ABOUT_PAGE_NUM] = {
    {.text = (char *)"- About K9-Pad"},
    {.text = (char *)"- FW: v0.2.0"},
    {.text = (char *)"- RMK Framework"},
    {.text = (char *)"- WouoUI Menu"}
};

//--------回调函数

// 主菜单回调：跳转到各子页面
static bool MainPage_Callback(const Page *cur_page, InputMsg msg) {
    if (msg != msg_click) return false;

    Option* opt = WouoUI_ListTitlePageGetSelectOpt(cur_page);
    if (opt == NULL) return false;

    switch (opt->order) {
        case 0: WouoUI_JumpToPage((PageAddr)cur_page, &pad_a_page); break;
        case 1: WouoUI_JumpToPage((PageAddr)cur_page, &pad_b_page); break;
        case 2: WouoUI_JumpToPage((PageAddr)cur_page, &pad_c_page); break;
        case 3: WouoUI_JumpToPage((PageAddr)cur_page, &pad_d_page); break;
        case 4: WouoUI_JumpToPage((PageAddr)cur_page, &pad_e_page); break;
        case 5: WouoUI_JumpToPage((PageAddr)cur_page, &user_page); break;
        case 6: WouoUI_JumpToPage((PageAddr)cur_page, &settings_page); break;
        case 7: WouoUI_JumpToPage((PageAddr)cur_page, &about_page); break;
    }
    return false;
}

// Layer 子页面回调（5 个 Layer 页面共用）
// 点击 Enable 后设置 Layer 并请求退出菜单
// 点击 Data Ch/Volume/Subs/Time 后同步 g_pad_dc_enabled
static bool PadPage_Callback(const Page *cur_page, InputMsg msg) {
    if (msg != msg_click) return false;

    Option* opt = WouoUI_ListTitlePageGetSelectOpt(cur_page);
    if (opt == NULL) return false;

    // 根据页面地址确定是哪个 Layer
    uint8_t pad_idx = 0;
    if ((PageAddr)cur_page == (PageAddr)&pad_b_page) {
        pad_idx = 1;
    } else if ((PageAddr)cur_page == (PageAddr)&pad_c_page) {
        pad_idx = 2;
    } else if ((PageAddr)cur_page == (PageAddr)&pad_d_page) {
        pad_idx = 3;
    } else if ((PageAddr)cur_page == (PageAddr)&pad_e_page) {
        pad_idx = 4;
    }

    if (opt->order == 1) {
        // "Enable" action: select this layer and request exit
        g_selected_pad = pad_idx;
        g_exit_requested = 1;
    }
    // Checkbox items (Volume, Subs, Time) at order 2-4
    // val is auto-toggled by WouoUI auto_deal_with_msg

    return false;
}

// User 页面回调
static bool UserPage_Callback(const Page *cur_page, InputMsg msg) {
    if (msg != msg_click) return false;

    Option* opt = WouoUI_ListTitlePageGetSelectOpt(cur_page);
    if (opt == NULL) return false;

    if (opt->order == 4) { // Clear Bond
        WouoUI_MsgWinPageSetContent(&msg_win, (char*)"Bond info cleared!");
        WouoUI_JumpToPage((PageAddr)cur_page, &msg_win);
    }
    // Radio items (order 1-3) are auto-handled by Setting_radio
    return false;
}

// DFU/Bootloader 确认弹窗回调 (auto_deal_with_msg=false, 手动处理所有消息)
static bool DFUConfWin_Callback(const Page *cur_page, InputMsg msg) {
    ConfWin *cw = (ConfWin *)cur_page;
    switch (msg) {
        case msg_up:
        case msg_down:
            WouoUI_ConfWinPageToggleBtn(cw);
            break;
        case msg_click:
            if (cw->conf_ret) { // User selected "Yes"
                if (pending_dfu_action == 1) {
                    g_dfu_requested = 1;
                    WouoUI_MsgWinPageSetContent(&msg_win, (char*)"Entering DFU...");
                } else {
                    g_usb_bl_requested = 1;
                    WouoUI_MsgWinPageSetContent(&msg_win, (char*)"To Bootloader...");
                }
                // Jump from Settings (not ConfWin) to MsgWin, so MsgWin returns to Settings
                WouoUI_JumpToPage(cur_page->last_page, &msg_win);
            } else { // User selected "No" - return to settings
                return true;
            }
            break;
        case msg_return:
            return true; // Return to settings
        default:
            break;
    }
    return false;
}

// Settings 页面回调
static bool SettingsPage_Callback(const Page *cur_page, InputMsg msg) {
    if (msg != msg_click) return false;

    Option* opt = WouoUI_ListTitlePageGetSelectOpt(cur_page);
    if (opt == NULL) return false;

    switch (opt->order) {
        case 1: // BLE toggle - auto handled by @ checkbox
            break;
        case 2: // Brightness - jump to ValWin
            WouoUI_JumpToPage((PageAddr)cur_page, &brightness_win);
            break;
        case 3: // Screen Off - jump to ListWin selector
            WouoUI_JumpToPage((PageAddr)cur_page, &screen_timeout_win);
            break;
        case 4: // Quick Menu toggle - auto handled by @ checkbox
            break;
        case 5: // DFU Mode - show confirmation dialog
            pending_dfu_action = 1;
            dfu_conf_win.content = (char*)"Enter DFU Mode?";
            dfu_conf_win.conf_ret = false; // Default to "No" for safety
            WouoUI_JumpToPage((PageAddr)cur_page, &dfu_conf_win);
            break;
        case 6: // Bootloader Mode - show confirmation dialog
            pending_dfu_action = 2;
            dfu_conf_win.content = (char*)"Enter Bootloader?";
            dfu_conf_win.conf_ret = false; // Default to "No" for safety
            WouoUI_JumpToPage((PageAddr)cur_page, &dfu_conf_win);
            break;
    }
    return false;
}

//--------初始化函数
void WouoUI_UserInit(void) {
    // 主菜单 (TitlePage with 8 icons)
    WouoUI_TitlePageInit(
        &main_page,
        MAIN_PAGE_NUM,
        main_option_array,
        main_icon_array,
        MainPage_Callback
    );

    // Layer 0/1/2/3/4 子页面
    WouoUI_ListPageInit(&pad_a_page, PAD_PAGE_NUM, pad_a_option_array, Setting_none, PadPage_Callback);
    WouoUI_ListPageSetFirstSelectable(&pad_a_page, 1);
    WouoUI_ListPageInit(&pad_b_page, PAD_PAGE_NUM, pad_b_option_array, Setting_none, PadPage_Callback);
    WouoUI_ListPageSetFirstSelectable(&pad_b_page, 1);
    WouoUI_ListPageInit(&pad_c_page, PAD_PAGE_NUM, pad_c_option_array, Setting_none, PadPage_Callback);
    WouoUI_ListPageSetFirstSelectable(&pad_c_page, 1);
    WouoUI_ListPageInit(&pad_d_page, PAD_PAGE_NUM, pad_d_option_array, Setting_none, PadPage_Callback);
    WouoUI_ListPageSetFirstSelectable(&pad_d_page, 1);
    WouoUI_ListPageInit(&pad_e_page, PAD_PAGE_NUM, pad_e_option_array, Setting_none, PadPage_Callback);
    WouoUI_ListPageSetFirstSelectable(&pad_e_page, 1);

    // User 页面 (radio buttons for BLE multi-device)
    WouoUI_ListPageInit(&user_page, USER_PAGE_NUM, user_option_array, Setting_radio, UserPage_Callback);

    // Settings 页面
    WouoUI_ListPageInit(&settings_page, SETTINGS_PAGE_NUM, settings_option_array, Setting_none, SettingsPage_Callback);

    // About 页面
    WouoUI_ListPageInit(&about_page, ABOUT_PAGE_NUM, about_option_array, Setting_none, NULL);

    // 共用消息弹窗
    WouoUI_MsgWinPageInit(&msg_win, NULL, false, 2, NULL);

    // DFU/Bootloader 确认弹窗 (auto_deal_with_msg=false, 由回调手动控制)
    WouoUI_ConfWinPageInit(&dfu_conf_win, NULL, NULL, NULL, false, false, false, 2, DFUConfWin_Callback);
    WouoUI_SetPageAutoDealWithMsg(&dfu_conf_win.page, false);

    // 亮度调节弹窗 (auto_get_bg_opt=true, auto_set_bg_opt=true)
    // 自动读写 settings_page 中 Brightness 选项的 val
    WouoUI_ValWinPageInit(&brightness_win, NULL, 80, 0, 100, 5, true, true, NULL);

    // 屏幕超时选择弹窗 (auto_set_bg_opt=true: 自动更新 settings "Screen Off" 的 .content)
    WouoUI_ListWinPageInit(&screen_timeout_win, 5, screen_timeout_options, true, NULL);
}

// Get selected layer index (0=Layer 0, 1=Layer 1, 2=Layer 2, 3=Layer 3, 4=Layer 4)
uint8_t WouoUI_K9Pad_GetSelectedPad(void) {
    return g_selected_pad;
}

// Set selected pad (for syncing menu state from external source)
void WouoUI_K9Pad_SetSelectedPad(uint8_t pad) {
    if (pad > 4) pad = 0;
    g_selected_pad = pad;
}

// Get brightness value (0-100) — confirmed value from settings option
uint8_t WouoUI_K9Pad_GetBrightness(void) {
    return (uint8_t)settings_option_array[2].val;
}

// Get live brightness value (0-100)
// Returns the real-time slider value when brightness ValWin is active,
// otherwise returns the confirmed value from settings.
uint8_t WouoUI_K9Pad_GetLiveBrightness(void) {
    if (p_cur_ui->current_page == (PageAddr)&brightness_win) {
        return (uint8_t)brightness_win.val;
    }
    return (uint8_t)settings_option_array[2].val;
}

// Set brightness value (0-100) — sets both settings option and ValWin
void WouoUI_K9Pad_SetBrightness(uint8_t val) {
    settings_option_array[2].val = val;
    brightness_win.val = val;
}

// Get BLE enabled state (1=on, 0=off)
uint8_t WouoUI_K9Pad_GetBleEnabled(void) {
    return (uint8_t)(settings_option_array[1].val != 0);
}

// Get selected user index (0=User A, 1=User B, 2=User C)
uint8_t WouoUI_K9Pad_GetSelectedUser(void) {
    for (uint8_t i = 1; i <= 3; i++) {
        if (user_option_array[i].val != 0) {
            return i - 1;
        }
    }
    return 0;
}

// Check if menu exit was requested (by pad selection etc.)
uint8_t WouoUI_K9Pad_GetExitRequested(void) {
    return g_exit_requested;
}

// Clear exit request flag
void WouoUI_K9Pad_ClearExitRequested(void) {
    g_exit_requested = 0;
}

// Get bitmask of enabled data channel functions for a pad
// Bit 1: Volume display
// Bit 2: Subscriber count
// Bit 3: Time display
uint16_t WouoUI_K9Pad_GetEnabledFunctions(uint8_t pad_index) {
    if (pad_index > 4) return 0;
    Option *opts;
    switch (pad_index) {
        case 0: opts = pad_a_option_array; break;
        case 1: opts = pad_b_option_array; break;
        case 2: opts = pad_c_option_array; break;
        case 3: opts = pad_d_option_array; break;
        case 4: opts = pad_e_option_array; break;
        default: return 0;
    }
    uint16_t mask = 0;
    if (opts[2].val) mask |= (1 << 1);  // Volume  -> bit 1
    if (opts[3].val) mask |= (1 << 2);  // Subs    -> bit 2
    if (opts[4].val) mask |= (1 << 3);  // Time    -> bit 3
    return mask;
}

// Set enabled data channel functions for a pad from bitmask
// Bit 1: Volume, Bit 2: Subs, Bit 3: Time
void WouoUI_K9Pad_SetEnabledFunctions(uint8_t pad_index, uint16_t mask) {
    if (pad_index > 4) return;
    Option *opts;
    switch (pad_index) {
        case 0: opts = pad_a_option_array; break;
        case 1: opts = pad_b_option_array; break;
        case 2: opts = pad_c_option_array; break;
        case 3: opts = pad_d_option_array; break;
        case 4: opts = pad_e_option_array; break;
        default: return;
    }
    opts[2].val = (mask & (1 << 1)) ? 1 : 0;  // Volume
    opts[3].val = (mask & (1 << 2)) ? 1 : 0;  // Subs
    opts[4].val = (mask & (1 << 3)) ? 1 : 0;  // Time
}

// Check if data channel is enabled for a pad (any function checkbox is checked)
uint8_t WouoUI_K9Pad_IsDataChannelEnabled(uint8_t pad_index) {
    return WouoUI_K9Pad_GetEnabledFunctions(pad_index) != 0 ? 1 : 0;
}

// Check if DFU mode was requested
uint8_t WouoUI_K9Pad_GetDFURequested(void) {
    return g_dfu_requested;
}

// Clear DFU request flag
void WouoUI_K9Pad_ClearDFURequested(void) {
    g_dfu_requested = 0;
}

// Check if USB bootloader mode was requested
uint8_t WouoUI_K9Pad_GetUSBBootloaderRequested(void) {
    return g_usb_bl_requested;
}

// Clear USB bootloader request flag
void WouoUI_K9Pad_ClearUSBBootloaderRequested(void) {
    g_usb_bl_requested = 0;
}

// Get Quick Menu enabled state (1=on, 0=off)
uint8_t WouoUI_K9Pad_GetQuickMenuEnabled(void) {
    return (uint8_t)(settings_option_array[4].val != 0);
}

// Set Quick Menu enabled state
void WouoUI_K9Pad_SetQuickMenuEnabled(uint8_t val) {
    settings_option_array[4].val = val ? 1 : 0;
}

// Get screen timeout in seconds from ListWin selection
// Maps: "5s"->5, "10s"->10, "20s"->20, "30s"->30, "1min"->60
uint8_t WouoUI_K9Pad_GetScreenTimeout(void) {
    static const uint8_t timeout_values[5] = {5, 10, 20, 30, 60};
    uint8_t idx = screen_timeout_win.sel_str_index;
    if (idx >= 5) idx = 2; // default to 20s
    return timeout_values[idx];
}

// Set screen timeout by seconds value
// Maps seconds to the corresponding ListWin index and updates .content
void WouoUI_K9Pad_SetScreenTimeout(uint8_t seconds) {
    uint8_t idx;
    switch (seconds) {
        case 5:  idx = 0; break;
        case 10: idx = 1; break;
        case 30: idx = 3; break;
        case 60: idx = 4; break;
        default: idx = 2; break; // 20s default
    }
    screen_timeout_win.sel_str_index = idx;
    // Update the Settings page option content to reflect current selection
    settings_option_array[3].content = screen_timeout_options[idx];
}
