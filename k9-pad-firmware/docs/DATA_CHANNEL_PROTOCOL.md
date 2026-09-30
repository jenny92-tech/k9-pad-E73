# K9-Pad E73 Data Channel Protocol

## 1. Overview

The K9-Pad E73 Data Channel provides a bidirectional communication link between a
host computer and the keyboard's OLED display. The design philosophy treats the
keyboard as a **remote display with menus** -- the host controls what data is
shown, while the keyboard handles rendering, animations, and user navigation.

Two transport options are supported:

- **BLE GATT** -- wireless, using a custom GATT service
- **USB CDC-ACM** -- wired, over a virtual serial port

Both transports use the same binary packet format, making the application layer
transport-agnostic.

---

## 2. Packet Format

Every packet uses a fixed 4-byte header followed by a variable-length payload:

```
+--------+--------+--------+--------+------------------------+
| CMD    | TYPE   | LEN-LO | LEN-HI | PAYLOAD (0..LEN bytes) |
| (1B)   | (1B)   | (1B)   | (1B)   |                        |
+--------+--------+--------+--------+------------------------+
```

| Field   | Size   | Description                                |
|---------|--------|--------------------------------------------|
| CMD     | 1 byte | Command identifier (see Section 3)         |
| TYPE    | 1 byte | Data type / subcommand (see Section 4)     |
| LEN     | 2 bytes (LE) | Payload length in bytes (excluding header) |
| PAYLOAD | 0..60 bytes | Command-specific data                  |

- **Maximum packet size:** 64 bytes (aligned with BLE characteristic size and USB CDC)
- **Header size:** 4 bytes
- **Maximum payload size:** 60 bytes
- All multi-byte integers are **little-endian**.

---

## 3. Command Reference

| CMD  | Name           | Direction       | Description                          |
|------|----------------|-----------------|--------------------------------------|
| 0x01 | SET_DISPLAY    | Host -> KB      | Push display data to a slot          |
| 0x02 | GET_STATUS     | Host -> KB      | Request current keyboard status      |
| 0x03 | STATUS_RESP    | KB -> Host      | Status response (pad config)         |
| 0x04 | CONFIG_CHANGED | KB -> Host      | User changed config in menu          |
| 0x05 | ACK            | KB -> Host      | Generic acknowledgement              |
| 0x10 | PING           | Bidirectional   | Heartbeat request                    |
| 0x11 | PONG           | Bidirectional   | Heartbeat response                   |
| 0x20 | SHOW_DIALOG    | Host -> KB      | Show confirm/cancel dialog (v2)      |
| 0x21 | DIALOG_RESULT  | KB -> Host      | User's dialog choice (v2)            |

### Command Details

**SET_DISPLAY (0x01)** -- Host pushes display data for a specific slot. The TYPE
field indicates the data format (text, numeric, progress, etc.). See Section 4 for
payload formats.

**GET_STATUS (0x02)** -- Host requests the keyboard's current configuration. The
keyboard responds with a STATUS_RESP packet.

**STATUS_RESP (0x03)** -- Keyboard sends its current pad configuration (active pad
index; `enabled_functions` is deprecated -- always `0xFFFF`). Sent in response
to GET_STATUS or unsolicited at boot.

**CONFIG_CHANGED (0x04)** -- Keyboard notifies the host that the user changed
the active Pad via the on-device menu. Contains the new PadConfig.

**ACK (0x05)** -- Generic acknowledgement. No payload.

**PING (0x10)** / **PONG (0x11)** -- Heartbeat pair. No payload. Either side can
send PING; the receiver replies with PONG.

**SHOW_DIALOG (0x20)** -- *(protocol v2)* Host asks the keyboard to show a
confirm/cancel dialog on the OLED (TYPE = TEXT). Payload:
`dialog_id(1B) + flags(1B, reserved=0) + UTF-8 text` (text up to 58 bytes,
ASCII only -- the firmware has no CJK font). The dialog preempts the current
screen, the user picks Yes/No with the encoder + confirm key, and SW1 backs
out as cancel. A new SHOW_DIALOG supersedes a pending one (the old one is
answered with Timeout). An unanswered dialog times out with the menu idle
timeout (~30 s).

**DIALOG_RESULT (0x21)** -- *(protocol v2)* Keyboard reports the outcome of a
dialog (TYPE = TEXT). Payload: `dialog_id(1B) + result(1B)` where result is
0 = Confirm, 1 = Cancel, 2 = Timeout. Hosts should check
`DeviceCapabilities.protocol_version >= 2` before sending SHOW_DIALOG; older
firmware silently discards unknown commands.

---

## 4. Data Type Reference

The TYPE byte in the header carries different meanings depending on the command.

### Display Data Types (used with SET_DISPLAY)

| TYPE | Name      | Payload Format                           | Payload Size       |
|------|-----------|------------------------------------------|--------------------|
| 0x01 | TEXT      | `slot_id(1B) + UTF-8 string`             | 1 + strlen         |
| 0x02 | NUMERIC   | `slot_id(1B) + i32 LE`                   | 5 bytes            |
| 0x03 | PROGRESS  | `slot_id(1B) + u8(0-100)`                | 2 bytes            |
| 0x04 | ICON_ID   | `slot_id(1B) + u16 LE`                   | 3 bytes            |
| 0x05 | KEY_VALUE | `slot_id(1B) + key_len(1B) + key + value`| 2 + key_len + val  |
| 0x06 | CLEAR     | `slot_id(1B)`                            | 1 byte             |

#### Payload Details

**TEXT (0x01):**
```
+----------+------------------------------+
| slot_id  | UTF-8 encoded string         |
| (1B)     | (up to 59 bytes)             |
+----------+------------------------------+
```

**NUMERIC (0x02):**
```
+----------+----------+----------+----------+----------+
| slot_id  | i32 byte 0 (LSB)  | byte 1   | byte 2   | byte 3 (MSB) |
| (1B)     |                                                         |
+----------+---------------------------------------------------------+
```

**PROGRESS (0x03):**
```
+----------+----------+
| slot_id  | percent  |
| (1B)     | (0-100)  |
+----------+----------+
```
Values above 100 are clamped to 100.

**ICON_ID (0x04):**
```
+----------+----------+----------+
| slot_id  | icon_id (u16 LE)   |
| (1B)     |                    |
+----------+----------+----------+
```

**KEY_VALUE (0x05):**
```
+----------+----------+------------------+------------------+
| slot_id  | key_len  | key (key_len B)  | value (remaining)|
| (1B)     | (1B)     |                  |                  |
+----------+----------+------------------+------------------+
```

**CLEAR (0x06):**
```
+----------+
| slot_id  |
| (1B)     |
+----------+
```
Clears the specified display slot.

### Config Data Types (used with CONFIG_CHANGED / STATUS_RESP)

| TYPE | Name       | Payload Format                                | Payload Size |
|------|------------|-----------------------------------------------|-------------|
| 0x10 | PAD_CONFIG | `active_pad(1B) + enabled_functions(2B LE)`   | 3 bytes     |

**PAD_CONFIG (0x10):**
```
+------------+----------+----------+
| active_pad | func_lo  | func_hi  |
| (1B)       | (1B)     | (1B)     |
+------------+----------+----------+
```
- `active_pad`: 0 = Pad A, 1 = Pad B, 2 = Pad C
- `enabled_functions`: **deprecated** -- 16-bit bitmask (see Section 5). Display
  is push-driven: the device shows whatever slots the host pushes. Firmware
  always reports `0xFFFF`; hosts must ignore this field. Kept for wire
  compatibility only.

---

## 5. Function Bitmask (deprecated)

The `enabled_functions` field in PadConfig is a 16-bit bitmask. **Deprecated:**
it no longer drives any decision on either side -- firmware reports `0xFFFF`
and hosts ignore it. The bit definitions below are retained only to document
the legacy wire format:

| Bit | Mask   | Name        | Description                   |
|-----|--------|-------------|-------------------------------|
| 0   | 0x0001 | FOLLOW_PC   | Follow computer state         |
| 1   | 0x0002 | VOLUME      | System volume display         |
| 2   | 0x0004 | SUBSCRIBERS | Subscriber count display      |
| 3   | 0x0008 | TIME        | Time display                  |

Bits 4-15 are reserved for future use and should be set to 0.

---

## 6. BLE Transport

The data channel uses a custom GATT service:

| Item              | UUID                                          |
|-------------------|-----------------------------------------------|
| Service           | `e9dc0001-7374-7265-616d-6b3970616400`        |
| RX Characteristic | `e9dc0002-7374-7265-616d-6b3970616400`        |
| TX Characteristic | `e9dc0003-7374-7265-616d-6b3970616400`        |

### Characteristics

**RX (Host -> Keyboard):**
- Properties: Write Without Response
- Used by the host to send commands (SET_DISPLAY, GET_STATUS, PING)
- Maximum write size: 64 bytes

**TX (Keyboard -> Host):**
- Properties: Read, Notify
- Used by the keyboard to send responses (STATUS_RESP, CONFIG_CHANGED, ACK, PONG, DIALOG_RESULT)
- Maximum notification size: 64 bytes

### MTU Requirement

A single 64-byte packet requires an ATT MTU of at least **67 bytes** (64 data +
3 ATT header). The host should request MTU negotiation during connection setup.
If the negotiated MTU is smaller, the application layer must handle fragmentation
(not currently implemented -- MTU >= 67 is assumed).

---

## 7. USB Transport

When connected via USB, the data channel uses a **CDC-ACM** (virtual serial port)
interface. This is the second CDC interface on the device (the first is used for
debug logging).

- Same binary packet format as BLE
- 64-byte packet alignment
- No additional framing -- each USB transfer carries exactly one packet
- Host identifies the correct CDC port by USB interface number or device enumeration order

---

## 8. Configuration Flow

The typical lifecycle of a data channel session:

```
  Host                                        Keyboard
   |                                             |
   |  <-- STATUS_RESP (PadConfig) --------------|  (1) Boot / connect
   |                                             |
   |  --- GET_STATUS ------------------------------>  (optional)
   |  <-- STATUS_RESP (PadConfig) --------------|
   |                                             |
   |  Subscribe to TX notifications (BLE)       |  (2) Setup
   |  or begin polling (USB)                    |
   |                                             |
   |                            User navigates   |  (3) Menu interaction
   |                            menu, switches   |
   |                            the active Pad   |
   |                                             |
   |  <-- CONFIG_CHANGED (new active_pad) ------|  (4) Notify host
   |                                             |
   |  Host updates UI (active pad display)      |  (5) Adjust
   |                                             |
   |  --- SET_DISPLAY (text/numeric/...) -------->  (6) Push data
   |  --- SET_DISPLAY (progress) ---------------->
   |  --- SET_DISPLAY (text) -------------------->
   |                                             |
   |  --- PING ---------------------------------->  (7) Periodic heartbeat
   |  <-- PONG ----------------------------------|
   |                                             |
```

**Step-by-step:**

1. On boot or BLE connection, the keyboard sends a `STATUS_RESP` with its current
   `PadConfig` (active pad; `enabled_functions` is always `0xFFFF`).
2. The host subscribes to TX characteristic notifications (BLE) or begins polling
   the CDC port (USB).
3. The user navigates the on-device menu and switches the active Pad.
4. The keyboard sends `CONFIG_CHANGED` with the new `active_pad`
   (`enabled_functions` remains `0xFFFF`).
5. The host uses the report for UI display only. Data providers (volume monitor,
   subscriber API poller, clock, ...) run unconditionally -- there is no
   device-side function gating.
6. The host pushes `SET_DISPLAY` packets for whatever slots it wants shown; the
   device displays any slot that has data.
7. Either side can send `PING`/`PONG` for connection health monitoring.

---

## 9. Display Slot Rotation

When the host pushes data to multiple slots simultaneously, the keyboard rotates
through the active slots on the OLED:

- Each function maps to a fixed **slot_id** (0-7)
- Default slot assignments:

| Slot | Function    |
|------|-------------|
| 0    | Volume      |
| 1    | Subscribers |
| 2    | Time        |
| 3-7  | Reserved    |

- A slot is "active" as soon as it has data; the keyboard cycles through active
  slots every **3-5 seconds** with animated transitions
- The host should keep all pushed slots up-to-date, regardless of which slot is
  currently displayed
- Sending `CLEAR` to a slot removes its content from the rotation

---

## 10. Example Packet Sequences

All values shown in hexadecimal. Multi-byte integers are little-endian.

### Push text "Vol: 75%" to slot 0

```
CMD: 01 (SET_DISPLAY)
TYPE: 01 (TEXT)
LEN:  09 00 (9 bytes: 1 slot + 8 chars)
PAYLOAD: 00 56 6F 6C 3A 20 37 35 25
              V  o  l  :     7  5  %

Full packet (13 bytes):
01 01 09 00 00 56 6F 6C 3A 20 37 35 25
```

### Push progress 75 to slot 0

```
CMD: 01 (SET_DISPLAY)
TYPE: 03 (PROGRESS)
LEN:  02 00 (2 bytes: 1 slot + 1 value)
PAYLOAD: 00 4B
            ^  ^
            |  75 decimal
            slot 0

Full packet (6 bytes):
01 03 02 00 00 4B
```

### Push numeric -12345 to slot 1

```
CMD: 01 (SET_DISPLAY)
TYPE: 02 (NUMERIC)
LEN:  05 00 (5 bytes: 1 slot + 4 i32)
PAYLOAD: 01 C7 CF FF FF
            ^  ^^^^^^^^
            |  -12345 as i32 LE
            slot 1

Full packet (9 bytes):
01 02 05 00 01 C7 CF FF FF
```

### Config changed: Pad B active

```
CMD: 04 (CONFIG_CHANGED)
TYPE: 10 (PAD_CONFIG)
LEN:  03 00 (3 bytes)
PAYLOAD: 01 FF FF
         ^  ^^^^^
         |  enabled_functions = 0xFFFF (deprecated field, always 0xFFFF)
         Pad B (index 1)

Full packet (7 bytes):
04 10 03 00 01 FF FF
```

### Status response: Pad A active

```
CMD: 03 (STATUS_RESP)
TYPE: 10 (PAD_CONFIG)
LEN:  03 00 (3 bytes)
PAYLOAD: 00 FF FF
         ^  ^^^^^
         |  enabled_functions = 0xFFFF (deprecated field, always 0xFFFF)
         Pad A (index 0)

Full packet (7 bytes):
03 10 03 00 00 FF FF
```

### Ping / Pong heartbeat

```
PING (4 bytes):
10 01 00 00

PONG (4 bytes):
11 01 00 00
```

Note: PING/PONG use TYPE=0x01 (TEXT) with zero-length payload as a convention.
The TYPE field is not meaningful for heartbeat commands.

### ACK (4 bytes)

```
05 01 00 00
```

### Show dialog "Reboot?" with dialog_id 7, user confirms

```
SHOW_DIALOG (13 bytes):
CMD: 20 (SHOW_DIALOG)
TYPE: 01 (TEXT)
LEN:  09 00 (9 bytes: 1 id + 1 flags + 7 chars)
PAYLOAD: 07 00 52 65 62 6F 6F 74 3F
              R  e  b  o  o  t  ?

Full packet (13 bytes):
20 01 09 00 07 00 52 65 62 6F 6F 74 3F

DIALOG_RESULT (6 bytes):
CMD: 21 (DIALOG_RESULT)
TYPE: 01 (TEXT)
LEN:  02 00
PAYLOAD: 07 00
            ^  ^
            |  0 = Confirm
            dialog_id 7

Full packet (6 bytes):
21 01 02 00 07 00
```

---

## 11. Error Handling

The protocol defines the following decode errors:

| Error             | Cause                                      |
|-------------------|--------------------------------------------|
| BufferTooShort    | Received fewer than 4 bytes                |
| UnknownCommand    | CMD byte does not match any known command  |
| UnknownDataType   | TYPE byte does not match any known type    |
| PayloadTooLarge   | LEN exceeds 60 bytes                       |

On receiving an invalid packet, the receiver silently discards it. No error
response is sent. The heartbeat mechanism (PING/PONG) serves as the connection
health indicator.

---

## 12. Rust Crate

The protocol is implemented in the `k9-datachannel-proto` crate, usable in both
`std` and `no_std` environments.

Key types and functions:

```rust
// Constants
const MAX_PACKET_SIZE: usize = 64;
const HEADER_SIZE: usize = 4;
const MAX_PAYLOAD_SIZE: usize = 60;

// Enums
enum CommandId { SetDisplay, GetStatus, StatusResp, ConfigChanged, Ack, Ping, Pong,
                 ShowDialog, DialogResult }
enum DataType { Text, Numeric, Progress, IconId, KeyValue, Clear, PadConfig }
enum DialogResultCode { Confirm = 0, Cancel = 1, Timeout = 2 }

// Structs
struct PacketHeader { cmd, data_type, payload_len }
struct PadConfig { active_pad, enabled_functions }

// Builder functions
fn build_packet(buf, cmd, data_type, payload) -> Option<usize>
fn build_set_text(buf, slot, text) -> Option<usize>
fn build_set_numeric(buf, slot, value) -> Option<usize>
fn build_set_progress(buf, slot, percent) -> Option<usize>
fn build_set_clear(buf, slot) -> Option<usize>
fn build_config_changed(buf, config) -> Option<usize>
fn build_status_resp(buf, config) -> Option<usize>
fn build_ping(buf) -> Option<usize>
fn build_pong(buf) -> Option<usize>
fn build_ack(buf) -> Option<usize>
fn build_show_dialog(buf, dialog_id, text) -> Option<usize>
fn build_dialog_result(buf, dialog_id, result) -> Option<usize>
```

All builder functions write into a caller-provided `&mut [u8]` buffer and return
the total number of bytes written (header + payload), or `None` if the buffer is
too small or the payload exceeds the maximum size.
