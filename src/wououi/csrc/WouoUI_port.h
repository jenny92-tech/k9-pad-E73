/**
 * WouoUI Port Layer for K9-Pad E73
 *
 * This file provides the interface between WouoUI and the Rust display driver.
 */

#ifndef __WOUOUI_PORT_H__
#define __WOUOUI_PORT_H__

#include "WouoUI.h"

#ifdef __cplusplus
extern "C" {
#endif

// Flag indicating buffer has been updated
extern volatile uint8_t g_screen_dirty;

// Menu state flags
extern volatile uint8_t g_menu_active;

// Initialize WouoUI for K9-Pad
void WouoUI_PortInit(void);

// Process one frame (call from Rust at desired FPS)
// Returns: 1 if screen was updated, 0 otherwise
uint8_t WouoUI_PortTick(uint16_t elapsed_ms);

// Send input to WouoUI
void WouoUI_PortSendInput(uint8_t input_type);

// Input types
#define INPUT_NONE      0
#define INPUT_UP        1
#define INPUT_DOWN      2
#define INPUT_LEFT      3
#define INPUT_RIGHT     4
#define INPUT_CLICK     5
#define INPUT_RETURN    6

// Get current menu state
uint8_t WouoUI_PortIsMenuActive(void);

// Enter/exit menu
void WouoUI_PortEnterMenu(void);
void WouoUI_PortExitMenu(void);

// Reset WouoUI to clean entry state (call before entering menu)
void WouoUI_PortResetForEntry(void);

// Get screen buffer pointer
uint8_t* WouoUI_PortGetBuffer(void);

// Get buffer size
uint16_t WouoUI_PortGetBufferSize(void);

// Check if currently on the home page
uint8_t WouoUI_PortIsOnHomePage(void);

// Configure animation timing for target frame interval (ms)
// Automatically adjusts FADE_ANI to maintain consistent blur coverage
// across different frame rates. Call once after PortInit.
void WouoUI_PortConfigFrameTime(uint16_t frame_ms);

// K9-Pad specific: Get selected layer index (0=Layer 0, 1=Layer 1, 2=Layer 2)
uint8_t WouoUI_K9Pad_GetSelectedPad(void);

// K9-Pad specific: Set selected pad (for syncing menu state)
void WouoUI_K9Pad_SetSelectedPad(uint8_t pad);

// K9-Pad specific: Get brightness value (0-100)
uint8_t WouoUI_K9Pad_GetBrightness(void);

// K9-Pad specific: Get BLE enabled state (1=on, 0=off)
uint8_t WouoUI_K9Pad_GetBleEnabled(void);

// K9-Pad specific: Get selected user index (0=User A, 1=User B, 2=User C)
uint8_t WouoUI_K9Pad_GetSelectedUser(void);

#ifdef __cplusplus
}
#endif

#endif // __WOUOUI_PORT_H__
