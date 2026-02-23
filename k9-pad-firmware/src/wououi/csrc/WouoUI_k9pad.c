// INPUT:  WouoUI.h
// OUTPUT: K9Pad_MenuInit() — NUM_LAYERS 主菜单项 + Features/User/Settings/About 子页面 + SetBrightness + ScreenTimeout + QuickMenu
// POS:    K9-Pad 专用菜单树定义，被 WouoUI_port.c 调用
/**
 * WouoUI K9-Pad Menu Configuration
 *
 * TitlePage main menu (NUM_LAYERS + 4 items):
 *   Layer 0..N-1 (direct switch) / User / Settings / Features / About
 *
 * Sub-pages:
 *   Features: tree menu → Layer 0..N-1 checkbox sub-pages (Volume/Subs/Time)
 *   User: User A/B/C radio (BLE multi-device) + Clear Bond
 *   Settings: Brightness slider + Screen Off selector + Quick Menu + DFU Mode + To Bootloader
 *   About: firmware info
 */

#include "WouoUI.h"

//--------Layer 数量 — 唯一真相源 (C 侧)
// SYNC: 必须与 mode.rs NUM_LAYERS 保持一致
#define NUM_LAYERS 5

//--------定义页面对象
static TitlePage main_page;
static ListPage pad_pages[NUM_LAYERS];
static ListPage features_page;
static ListPage user_page;
static ListPage settings_page;
static ListPage about_page;
static MsgWin msg_win;
static ConfWin dfu_conf_win;
static ValWin brightness_win;
static ListWin screen_timeout_win;

//--------DFU/Bootloader 确认弹窗的 pending action (1=DFU, 2=Bootloader)
static uint8_t pending_dfu_action = 0;

//--------当前选中的 Layer (0..NUM_LAYERS-1)
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


//--------页面选项数量 (由 NUM_LAYERS 派生)
#define MAIN_PAGE_NUM       (NUM_LAYERS + 4)  // layers + User + Settings + Features + About
#define PAD_PAGE_NUM        4                  // 标题 + Volume + Subs + Time (不随 layer 数变)
#define FEATURES_PAGE_NUM   (NUM_LAYERS + 1)   // 标题 + N 个 Layer 入口
#define USER_PAGE_NUM       5
#define SETTINGS_PAGE_NUM   6
#define ABOUT_PAGE_NUM      4

//--------文本查找表 (gen_layer_data.py 生成，与 NUM_LAYERS 同步)
static char* main_layer_texts[NUM_LAYERS] = {
    (char*)"+ Layer 0", (char*)"+ Layer 1", (char*)"+ Layer 2",
    (char*)"+ Layer 3", (char*)"+ Layer 4"
};

//--------主菜单选项 (WouoUI_UserInit 中填充)
static Option main_option_array[MAIN_PAGE_NUM];

//--------数字图标查找表 (30x30, gen_layer_data.py 生成，与 NUM_LAYERS 同步)
// 每个图标 120 bytes (30×4 rows), 运行时由 WouoUI_UserInit 复制到 main_icon_storage
// 增加 Layer 时用 tools/gen_layer_data.py 重新生成此数组
static const Icon layer_digit_icons[NUM_LAYERS] = {
    // [0] digit "0"
    {
        0x00, 0x00, 0xF8, 0xFC, 0xFE, 0x0E, 0x06, 0x06, 0x06, 0x06, 0x06, 0x06, 0x06, 0x06, 0x06, 0x06, 0x06, 0x06, 0x06, 0x06, 0x06, 0x06, 0x06, 0x0E, 0xFE, 0xFC, 0xF8, 0x00, 0x00, 0x00,
        0x00, 0x00, 0xFF, 0xFF, 0xFF, 0x00, 0x00, 0x00, 0x00, 0xFE, 0xFF, 0x03, 0x03, 0x03, 0x03, 0x03, 0x03, 0xFF, 0xFE, 0x00, 0x00, 0x00, 0x00, 0x00, 0xFF, 0xFF, 0xFF, 0x00, 0x00, 0x00,
        0x00, 0x00, 0xFF, 0xFF, 0xFF, 0x00, 0x00, 0x00, 0x00, 0x7F, 0xFF, 0xC0, 0xC0, 0xC0, 0xC0, 0xC0, 0xC0, 0xFF, 0x7F, 0x00, 0x00, 0x00, 0x00, 0x00, 0xFF, 0xFF, 0xFF, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x1F, 0x3F, 0x7F, 0x70, 0x60, 0x60, 0x60, 0x60, 0x60, 0x60, 0x60, 0x60, 0x60, 0x60, 0x60, 0x60, 0x60, 0x60, 0x60, 0x60, 0x60, 0x70, 0x7F, 0x3F, 0x1F, 0x00, 0x00, 0x00
    },
    // [1] digit "1"
    {
        0x00, 0x00, 0xF8, 0xFC, 0xFE, 0x0E, 0x06, 0x06, 0x06, 0x06, 0x06, 0x06, 0x06, 0x06, 0x06, 0x06, 0x06, 0x06, 0x06, 0x06, 0x06, 0x06, 0x06, 0x0E, 0xFE, 0xFC, 0xF8, 0x00, 0x00, 0x00,
        0x00, 0x00, 0xFF, 0xFF, 0xFF, 0x00, 0x00, 0x00, 0x00, 0x00, 0x08, 0x0C, 0x0E, 0xFF, 0xFF, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0xFF, 0xFF, 0xFF, 0x00, 0x00, 0x00,
        0x00, 0x00, 0xFF, 0xFF, 0xFF, 0x00, 0x00, 0x00, 0x00, 0x00, 0xC0, 0xC0, 0xC0, 0xFF, 0xFF, 0xC0, 0xC0, 0xC0, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0xFF, 0xFF, 0xFF, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x1F, 0x3F, 0x7F, 0x70, 0x60, 0x60, 0x60, 0x60, 0x60, 0x60, 0x60, 0x60, 0x60, 0x60, 0x60, 0x60, 0x60, 0x60, 0x60, 0x60, 0x60, 0x70, 0x7F, 0x3F, 0x1F, 0x00, 0x00, 0x00
    },
    // [2] digit "2"
    {
        0x00, 0x00, 0xF8, 0xFC, 0xFE, 0x0E, 0x06, 0x06, 0x06, 0x06, 0x06, 0x06, 0x06, 0x06, 0x06, 0x06, 0x06, 0x06, 0x06, 0x06, 0x06, 0x06, 0x06, 0x0E, 0xFE, 0xFC, 0xF8, 0x00, 0x00, 0x00,
        0x00, 0x00, 0xFF, 0xFF, 0xFF, 0x00, 0x00, 0x00, 0x00, 0x06, 0x07, 0x03, 0x83, 0x83, 0xC3, 0xC3, 0xE3, 0x7F, 0x3E, 0x00, 0x00, 0x00, 0x00, 0x00, 0xFF, 0xFF, 0xFF, 0x00, 0x00, 0x00,
        0x00, 0x00, 0xFF, 0xFF, 0xFF, 0x00, 0x00, 0x00, 0x00, 0xFE, 0xFF, 0xC3, 0xC3, 0xC1, 0xC1, 0xC0, 0xC0, 0xC0, 0xC0, 0x00, 0x00, 0x00, 0x00, 0x00, 0xFF, 0xFF, 0xFF, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x1F, 0x3F, 0x7F, 0x70, 0x60, 0x60, 0x60, 0x60, 0x60, 0x60, 0x60, 0x60, 0x60, 0x60, 0x60, 0x60, 0x60, 0x60, 0x60, 0x60, 0x60, 0x70, 0x7F, 0x3F, 0x1F, 0x00, 0x00, 0x00
    },
    // [3] digit "3"
    {
        0x00, 0x00, 0xF8, 0xFC, 0xFE, 0x0E, 0x06, 0x06, 0x06, 0x06, 0x06, 0x06, 0x06, 0x06, 0x06, 0x06, 0x06, 0x06, 0x06, 0x06, 0x06, 0x06, 0x06, 0x0E, 0xFE, 0xFC, 0xF8, 0x00, 0x00, 0x00,
        0x00, 0x00, 0xFF, 0xFF, 0xFF, 0x00, 0x00, 0x00, 0x00, 0x06, 0x07, 0x03, 0x03, 0xC3, 0xC3, 0xC3, 0xC3, 0xFF, 0x3E, 0x00, 0x00, 0x00, 0x00, 0x00, 0xFF, 0xFF, 0xFF, 0x00, 0x00, 0x00,
        0x00, 0x00, 0xFF, 0xFF, 0xFF, 0x00, 0x00, 0x00, 0x00, 0x60, 0xE0, 0xC0, 0xC0, 0xC0, 0xC0, 0xC0, 0xC0, 0xFF, 0x7F, 0x00, 0x00, 0x00, 0x00, 0x00, 0xFF, 0xFF, 0xFF, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x1F, 0x3F, 0x7F, 0x70, 0x60, 0x60, 0x60, 0x60, 0x60, 0x60, 0x60, 0x60, 0x60, 0x60, 0x60, 0x60, 0x60, 0x60, 0x60, 0x60, 0x60, 0x70, 0x7F, 0x3F, 0x1F, 0x00, 0x00, 0x00
    },
    // [4] digit "4"
    {
        0x00, 0x00, 0xF8, 0xFC, 0xFE, 0x0E, 0x06, 0x06, 0x06, 0x06, 0x06, 0x06, 0x06, 0x06, 0x06, 0x06, 0x06, 0x06, 0x06, 0x06, 0x06, 0x06, 0x06, 0x0E, 0xFE, 0xFC, 0xF8, 0x00, 0x00, 0x00,
        0x00, 0x00, 0xFF, 0xFF, 0xFF, 0x00, 0x00, 0x00, 0xC0, 0xE0, 0x30, 0x18, 0x0C, 0xFE, 0xFF, 0xFF, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0xFF, 0xFF, 0xFF, 0x00, 0x00, 0x00,
        0x00, 0x00, 0xFF, 0xFF, 0xFF, 0x00, 0x00, 0x00, 0x03, 0x03, 0x03, 0x03, 0x03, 0xFF, 0xFF, 0xFF, 0x03, 0x03, 0x03, 0x00, 0x00, 0x00, 0x00, 0x00, 0xFF, 0xFF, 0xFF, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x1F, 0x3F, 0x7F, 0x70, 0x60, 0x60, 0x60, 0x60, 0x60, 0x60, 0x60, 0x60, 0x60, 0x60, 0x60, 0x60, 0x60, 0x60, 0x60, 0x60, 0x60, 0x70, 0x7F, 0x3F, 0x1F, 0x00, 0x00, 0x00
    }
};

//--------固定菜单项图标 (User, Settings, Features, About)
static const Icon fixed_icons[4] = {
    // [0] User - Home icon
    {
        0x00, 0x00, 0x00, 0x00, 0x00, 0x80, 0xC0, 0xE0, 0xF0, 0xF8, 0xFC, 0xFE, 0xFF, 0xFE, 0xFC,
        0xFC, 0xFE, 0xFF, 0xFE, 0xFC, 0xF8, 0xF0, 0xE0, 0xC0, 0x80, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x00, 0xF0, 0xFE, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF,
        0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFE, 0xF0, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x0F, 0x7F, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0x0F, 0x0F, 0x0F, 0x0F, 0x0F,
        0x0F, 0x0F, 0x0F, 0x0F, 0x0F, 0xFF, 0xFF, 0xFF, 0xFF, 0x7F, 0x0F, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x00, 0x00, 0x01, 0x03, 0x07, 0x0F, 0x1F, 0x1C, 0x1C, 0x1C, 0x1C, 0x1C,
        0x1C, 0x1C, 0x1C, 0x1C, 0x1C, 0x1F, 0x0F, 0x07, 0x03, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00
    },
    // [1] Settings - Gear icon
    {
        0x00, 0x00, 0x00, 0x00, 0x80, 0xE0, 0xC0, 0x80, 0x00, 0x00, 0x80, 0x8C, 0xFC, 0xFC, 0xFC, 0xFC, 0xFC, 0xFC, 0x8C, 0x80, 0x00, 0x00, 0x80, 0xC0, 0xE0, 0x80, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x0C, 0x1F, 0x1F, 0x1F, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0x3F, 0x0F, 0x0F, 0x07, 0x07, 0x0F, 0x0F, 0x3F, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0x1F, 0x1F, 0x1F, 0x0C, 0x00, 0x00,
        0x00, 0x00, 0x0C, 0x3E, 0x7E, 0xFE, 0xFF, 0x7F, 0x3F, 0x3F, 0x7F, 0x7F, 0xFC, 0xFC, 0xF8, 0xF8, 0xFC, 0xFC, 0x7F, 0x7F, 0x3F, 0x3F, 0x7F, 0xFF, 0xFE, 0x7E, 0x3E, 0x0C, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x0C, 0x0F, 0x0F, 0x0F, 0x0F, 0x0F, 0x0F, 0x0C, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00
    },
    // [2] Features - Checklist icon
    {
        0x00, 0x00, 0xF8, 0xFC, 0xFE, 0x0E, 0x06, 0x06, 0x06, 0x06, 0x06, 0x06, 0x06, 0x06, 0x06, 0x06, 0x06, 0x06, 0x06, 0x06, 0x06, 0x06, 0x06, 0x0E, 0xFE, 0xFC, 0xF8, 0x00, 0x00, 0x00,
        0x00, 0x00, 0xFF, 0xFF, 0xFF, 0x00, 0x00, 0x18, 0x0C, 0x86, 0xC0, 0x60, 0x00, 0xFE, 0xFE, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0xFF, 0xFF, 0xFF, 0x00, 0x00, 0x00,
        0x00, 0x00, 0xFF, 0xFF, 0xFF, 0x00, 0x00, 0x18, 0x0C, 0x86, 0xC0, 0x60, 0x00, 0xFE, 0xFE, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0xFF, 0xFF, 0xFF, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x1F, 0x3F, 0x7F, 0x70, 0x60, 0x60, 0x60, 0x61, 0x63, 0x66, 0x60, 0x67, 0x67, 0x60, 0x60, 0x60, 0x60, 0x60, 0x60, 0x60, 0x60, 0x70, 0x7F, 0x3F, 0x1F, 0x00, 0x00, 0x00
    },
    // [3] About - Info icon
    {
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

//--------主菜单图标 (WouoUI_UserInit 中填充)
// 声明为 uint8_t 以支持运行时 memcpy，传入 TitlePageInit 时强转为 Icon*
static uint8_t main_icon_storage[MAIN_PAGE_NUM][ICON_BUFFSIZE];

//--------Layer 子页面选项文本查找表 (gen_layer_data.py 生成，与 NUM_LAYERS 同步)
static char* pad_titles[NUM_LAYERS] = {
    (char*)"- Layer 0", (char*)"- Layer 1", (char*)"- Layer 2",
    (char*)"- Layer 3", (char*)"- Layer 4"
};
static char* features_layer_texts[NUM_LAYERS] = {
    (char*)"! Layer 0", (char*)"! Layer 1", (char*)"! Layer 2",
    (char*)"! Layer 3", (char*)"! Layer 4"
};

//--------Layer 子页面选项 (Features → Layer N, WouoUI_UserInit 中初始化)
static Option pad_option_arrays[NUM_LAYERS][PAD_PAGE_NUM];

//--------Features 页面选项 (WouoUI_UserInit 中填充)
static Option features_option_array[FEATURES_PAGE_NUM];

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
    {.text = (char *)"~ Brightness", .val = 80},
    {.text = (char *)"> Screen Off", .content = (char*)"20s"},
    {.text = (char *)"@ Quick Menu", .val = 1},
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

// Features 页面回调：跳转到对应 Layer 的功能配置子页面
static bool FeaturesPage_Callback(const Page *cur_page, InputMsg msg) {
    if (msg != msg_click) return false;

    Option* opt = WouoUI_ListTitlePageGetSelectOpt(cur_page);
    if (opt == NULL) return false;

    // order 1..NUM_LAYERS → pad_pages[0..NUM_LAYERS-1]
    if (opt->order >= 1 && opt->order <= NUM_LAYERS) {
        WouoUI_JumpToPage((PageAddr)cur_page, &pad_pages[opt->order - 1]);
    }
    return false;
}

// 主菜单回调：Layer 0-4 直接切换并退出，其余跳转子页面
static bool MainPage_Callback(const Page *cur_page, InputMsg msg) {
    if (msg != msg_click) return false;

    Option* opt = WouoUI_ListTitlePageGetSelectOpt(cur_page);
    if (opt == NULL) return false;

    if (opt->order < NUM_LAYERS) {
        // Layer 选择 — 直接切换并退出菜单
        g_selected_pad = opt->order;
        g_exit_requested = 1;
    } else {
        // 固定菜单项 (User, Settings, Features, About)
        uint8_t fixed_idx = opt->order - NUM_LAYERS;
        switch (fixed_idx) {
            case 0: WouoUI_JumpToPage((PageAddr)cur_page, &user_page); break;
            case 1: WouoUI_JumpToPage((PageAddr)cur_page, &settings_page); break;
            case 2: WouoUI_JumpToPage((PageAddr)cur_page, &features_page); break;
            case 3: WouoUI_JumpToPage((PageAddr)cur_page, &about_page); break;
        }
    }
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
        case 1: // Brightness - jump to ValWin
            WouoUI_JumpToPage((PageAddr)cur_page, &brightness_win);
            break;
        case 2: // Screen Off - jump to ListWin selector
            WouoUI_JumpToPage((PageAddr)cur_page, &screen_timeout_win);
            break;
        case 3: // Quick Menu toggle - auto handled by @ checkbox
            break;
        case 4: // DFU Mode - show confirmation dialog
            pending_dfu_action = 1;
            dfu_conf_win.content = (char*)"Enter DFU Mode?";
            dfu_conf_win.conf_ret = false; // Default to "No" for safety
            WouoUI_JumpToPage((PageAddr)cur_page, &dfu_conf_win);
            break;
        case 5: // Bootloader Mode - show confirmation dialog
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
    // 生成主菜单 Layer 项 (前 NUM_LAYERS 个)
    for (uint8_t i = 0; i < NUM_LAYERS; i++) {
        main_option_array[i] = (Option){.text = main_layer_texts[i]};
        memcpy(main_icon_storage[i], layer_digit_icons[i], ICON_BUFFSIZE);
    }
    // 固定菜单项 (User, Settings, Features, About)
    main_option_array[NUM_LAYERS + 0] = (Option){.text = (char*)"+ User"};
    main_option_array[NUM_LAYERS + 1] = (Option){.text = (char*)"+ Settings"};
    main_option_array[NUM_LAYERS + 2] = (Option){.text = (char*)"+ Features"};
    main_option_array[NUM_LAYERS + 3] = (Option){.text = (char*)"+ About"};
    memcpy(main_icon_storage[NUM_LAYERS + 0], fixed_icons[0], ICON_BUFFSIZE);
    memcpy(main_icon_storage[NUM_LAYERS + 1], fixed_icons[1], ICON_BUFFSIZE);
    memcpy(main_icon_storage[NUM_LAYERS + 2], fixed_icons[2], ICON_BUFFSIZE);
    memcpy(main_icon_storage[NUM_LAYERS + 3], fixed_icons[3], ICON_BUFFSIZE);

    // 主菜单 (TitlePage)
    WouoUI_TitlePageInit(
        &main_page,
        MAIN_PAGE_NUM,
        main_option_array,
        (Icon *)main_icon_storage,
        MainPage_Callback
    );

    // 生成 Features 页面选项
    features_option_array[0] = (Option){.text = (char*)"- Features"};
    for (uint8_t i = 0; i < NUM_LAYERS; i++) {
        features_option_array[i + 1] = (Option){.text = features_layer_texts[i]};
    }

    // 生成 Pad 功能配置子页面
    for (uint8_t i = 0; i < NUM_LAYERS; i++) {
        pad_option_arrays[i][0] = (Option){.text = pad_titles[i]};
        pad_option_arrays[i][1] = (Option){.text = (char*)"@ Volume", .val = 0};
        pad_option_arrays[i][2] = (Option){.text = (char*)"@ Subs",   .val = 0};
        pad_option_arrays[i][3] = (Option){.text = (char*)"@ Time",   .val = 0};
        WouoUI_ListPageInit(&pad_pages[i], PAD_PAGE_NUM, pad_option_arrays[i], Setting_none, NULL);
        WouoUI_ListPageSetFirstSelectable(&pad_pages[i], 1);
    }

    // Features 页面 (树状菜单入口)
    WouoUI_ListPageInit(&features_page, FEATURES_PAGE_NUM, features_option_array, Setting_none, FeaturesPage_Callback);
    WouoUI_ListPageSetFirstSelectable(&features_page, 1);

    // User 页面 (radio buttons for BLE multi-device)
    WouoUI_ListPageInit(&user_page, USER_PAGE_NUM, user_option_array, Setting_radio, UserPage_Callback);
    WouoUI_ListPageSetFirstSelectable(&user_page, 1);

    // Settings 页面
    WouoUI_ListPageInit(&settings_page, SETTINGS_PAGE_NUM, settings_option_array, Setting_none, SettingsPage_Callback);
    WouoUI_ListPageSetFirstSelectable(&settings_page, 1);

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

// Get selected layer index (0..NUM_LAYERS-1)
uint8_t WouoUI_K9Pad_GetSelectedPad(void) {
    return g_selected_pad;
}

// Set selected pad (for syncing menu state from external source)
void WouoUI_K9Pad_SetSelectedPad(uint8_t pad) {
    if (pad >= NUM_LAYERS) pad = 0;
    g_selected_pad = pad;
}

// Get brightness value (0-100) — confirmed value from settings option
uint8_t WouoUI_K9Pad_GetBrightness(void) {
    return (uint8_t)settings_option_array[1].val;
}

// Get live brightness value (0-100)
// Returns the real-time slider value when brightness ValWin is active,
// otherwise returns the confirmed value from settings.
uint8_t WouoUI_K9Pad_GetLiveBrightness(void) {
    if (p_cur_ui->current_page == (PageAddr)&brightness_win) {
        return (uint8_t)brightness_win.val;
    }
    return (uint8_t)settings_option_array[1].val;
}

// Set brightness value (0-100) — sets both settings option and ValWin
void WouoUI_K9Pad_SetBrightness(uint8_t val) {
    settings_option_array[1].val = val;
    brightness_win.val = val;
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
    if (pad_index >= NUM_LAYERS) return 0;
    Option *opts = pad_option_arrays[pad_index];
    uint16_t mask = 0;
    if (opts[1].val) mask |= (1 << 1);  // Volume  -> bit 1
    if (opts[2].val) mask |= (1 << 2);  // Subs    -> bit 2
    if (opts[3].val) mask |= (1 << 3);  // Time    -> bit 3
    return mask;
}

// Set enabled data channel functions for a pad from bitmask
// Bit 1: Volume, Bit 2: Subs, Bit 3: Time
void WouoUI_K9Pad_SetEnabledFunctions(uint8_t pad_index, uint16_t mask) {
    if (pad_index >= NUM_LAYERS) return;
    Option *opts = pad_option_arrays[pad_index];
    opts[1].val = (mask & (1 << 1)) ? 1 : 0;  // Volume
    opts[2].val = (mask & (1 << 2)) ? 1 : 0;  // Subs
    opts[3].val = (mask & (1 << 3)) ? 1 : 0;  // Time
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
    return (uint8_t)(settings_option_array[3].val != 0);
}

// Set Quick Menu enabled state
void WouoUI_K9Pad_SetQuickMenuEnabled(uint8_t val) {
    settings_option_array[3].val = val ? 1 : 0;
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
    settings_option_array[2].content = screen_timeout_options[idx];
}
