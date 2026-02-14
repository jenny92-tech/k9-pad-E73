#!/usr/bin/env python3
# INPUT:  firmware.bin
# OUTPUT: dfu_package.zip (Adafruit SDK 11 format)
# POS:    生成 BLE OTA DFU 升级包，纯 Python 无依赖
"""Generate Adafruit nRF52 DFU package (SDK 11 format) without adafruit-nrfutil."""

import struct
import json
import zipfile
import sys
import os

def crc16_ccitt(data: bytes) -> int:
    """CRC-16/CCITT (same as used by Nordic SDK 11 DFU)."""
    crc = 0xFFFF
    for byte in data:
        crc = (crc >> 8 & 0x00FF) | (crc << 8 & 0xFF00)
        crc ^= byte
        crc ^= (crc & 0x00FF) >> 4
        crc ^= (crc << 8) << 4 & 0xFFFF
        crc ^= ((crc & 0x00FF) << 4) << 1 & 0xFFFF
    return crc & 0xFFFF

def gen_init_packet(firmware: bytes, dev_type=0x0052, dev_rev=0xFFFF, app_ver=0xFFFFFFFF, sd_req=0xFFFE) -> bytes:
    """Generate SDK 11 DFU init packet (.dat file)."""
    # Format: dev_type(u16) + dev_rev(u16) + app_ver(u32) + sd_req_count(u16) + sd_req(u16) + crc(u16)
    pkt = struct.pack('<HHI', dev_type, dev_rev, app_ver)
    pkt += struct.pack('<HH', 1, sd_req)  # 1 softdevice requirement entry
    fw_crc = crc16_ccitt(firmware)
    pkt += struct.pack('<H', fw_crc)
    return pkt

def main():
    if len(sys.argv) < 3:
        print(f"Usage: {sys.argv[0]} <firmware.bin> <output.zip>")
        sys.exit(1)

    fw_path = sys.argv[1]
    out_path = sys.argv[2]

    with open(fw_path, 'rb') as f:
        firmware = f.read()

    print(f"Firmware: {fw_path} ({len(firmware)} bytes)")
    fw_crc = crc16_ccitt(firmware)
    print(f"CRC-16: 0x{fw_crc:04X}")

    init_packet = gen_init_packet(firmware)
    print(f"Init packet: {len(init_packet)} bytes")

    # Create zip with manifest
    bin_name = "application.bin"
    dat_name = "application.dat"

    manifest = {
        "manifest": {
            "application": {
                "bin_file": bin_name,
                "dat_file": dat_name
            }
        }
    }

    with zipfile.ZipFile(out_path, 'w', zipfile.ZIP_DEFLATED) as zf:
        zf.writestr(bin_name, firmware)
        zf.writestr(dat_name, init_packet)
        zf.writestr("manifest.json", json.dumps(manifest, indent=2))

    print(f"DFU package: {out_path} ({os.path.getsize(out_path)} bytes)")
    print("Done! Transfer this .zip to your phone and use nRF Connect app to upload.")

if __name__ == '__main__':
    main()
