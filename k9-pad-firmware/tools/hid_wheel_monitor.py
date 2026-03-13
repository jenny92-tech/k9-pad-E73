#!/usr/bin/env python3
"""Monitor mouse wheel HID reports from K9-Pad.

Usage: python3 tools/hid_wheel_monitor.py

Connects to K9-Pad (VID=0x4C4B, PID=0x4643) and prints mouse reports.
Rotate encoder (mapped to WheelUp/Down in Vial) to see wheel values.
Press Ctrl+C to exit.
"""

import hid
import sys

VID = 0x4C4B
PID = 0x4643

# Mouse report: report_id=0x01, buttons, x, y, wheel, pan
MOUSE_REPORT_ID = 0x01

def main():
    print(f"Searching for K9-Pad (VID={VID:#06x}, PID={PID:#06x})...")

    # List all matching devices
    devices = hid.enumerate(VID, PID)
    if not devices:
        print("Device not found. Is it connected via USB?")
        sys.exit(1)

    print(f"Found {len(devices)} interface(s):")
    for d in devices:
        print(f"  path={d['path']}")
        print(f"  usage_page={d['usage_page']:#06x} usage={d['usage']:#06x}")
        print(f"  interface={d['interface_number']}")
        print()

    # Try to open the mouse/composite interface (usage_page=0x01, usage=0x02 for mouse)
    target = None
    for d in devices:
        if d['usage_page'] == 0x01 and d['usage'] == 0x02:
            target = d
            break

    # Fallback: try any generic desktop usage
    if not target:
        for d in devices:
            if d['usage_page'] == 0x01:
                target = d
                break

    # Fallback: just try the first one
    if not target:
        target = devices[0]

    print(f"Opening: usage_page={target['usage_page']:#06x} usage={target['usage']:#06x}")

    try:
        h = hid.device()
        h.open_path(target['path'])
        h.set_nonblocking(False)

        print("Connected! Rotate encoder to see wheel values. Ctrl+C to exit.\n")
        print("Format: [report_id] buttons x y WHEEL pan")
        print("-" * 50)

        while True:
            data = h.read(64, timeout_ms=5000)
            if data:
                if len(data) >= 5:
                    report_id = data[0] if len(data) > 5 else MOUSE_REPORT_ID
                    buttons = data[0]
                    x = data[1] if data[1] < 128 else data[1] - 256
                    y = data[2] if data[2] < 128 else data[2] - 256
                    wheel = data[3] if data[3] < 128 else data[3] - 256
                    pan = data[4] if data[4] < 128 else data[4] - 256

                    if wheel != 0:
                        print(f"  buttons={buttons:#04x} x={x:+4d} y={y:+4d} WHEEL={wheel:+4d} pan={pan:+4d}  <-- SCROLL")
                    else:
                        print(f"  buttons={buttons:#04x} x={x:+4d} y={y:+4d} wheel={wheel:+4d} pan={pan:+4d}")
                else:
                    print(f"  raw: {[hex(b) for b in data]}")

    except KeyboardInterrupt:
        print("\nDone.")
    except Exception as e:
        print(f"Error: {e}")
        print("Note: on macOS, you may need to run with sudo for HID access.")
    finally:
        try:
            h.close()
        except:
            pass

if __name__ == "__main__":
    main()
