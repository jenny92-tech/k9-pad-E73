#!/usr/bin/env python3
# INPUT:  firmware.bin (with FIRMWARE_INTEGRITY magic pattern)
# OUTPUT: firmware.bin (patched with CRC32 + size)
# POS:    构建后处理：为固件 bin 补丁 CRC32 完整性校验值
"""Patch firmware binary with CRC32 for boot-time integrity verification.

Usage:
    python3 tools/patch_crc.py <firmware.bin> [output.bin]

If output.bin is omitted, patches the file in-place.

The script searches for the FIRMWARE_INTEGRITY struct by its magic pattern,
computes CRC32 (with the crc32+size fields zeroed), and writes back.
"""

import struct
import sys
import zlib

MAGIC_HEAD = 0x4B394352  # "K9CR"
MAGIC_TAIL = 0x5243394B  # "RC9K"
UNPATCHED = 0xFFFFFFFF


def find_integrity_struct(data: bytes) -> int:
    """Find the FirmwareIntegrity struct by magic pattern."""
    # Unpatched pattern: HEAD + FF*8 + TAIL
    unpatched = struct.pack('<IIII', MAGIC_HEAD, UNPATCHED, UNPATCHED, MAGIC_TAIL)
    offset = data.find(unpatched)
    if offset != -1:
        return offset

    # Already-patched: HEAD + any 8 bytes + TAIL (for re-patching)
    for i in range(0, len(data) - 15):
        head, = struct.unpack_from('<I', data, i)
        if head != MAGIC_HEAD:
            continue
        tail, = struct.unpack_from('<I', data, i + 12)
        if tail == MAGIC_TAIL:
            return i

    return -1


def main():
    if len(sys.argv) < 2:
        print(f"Usage: {sys.argv[0]} <firmware.bin> [output.bin]")
        sys.exit(1)

    input_path = sys.argv[1]
    output_path = sys.argv[2] if len(sys.argv) > 2 else input_path

    with open(input_path, 'rb') as f:
        data = bytearray(f.read())

    fw_size = len(data)
    print(f"Firmware: {input_path} ({fw_size} bytes, {fw_size / 1024:.1f} KB)")

    offset = find_integrity_struct(data)
    if offset == -1:
        print("ERROR: FIRMWARE_INTEGRITY struct not found!")
        print("Make sure 'mod integrity' is included in the firmware.")
        sys.exit(1)

    print(f"Found FIRMWARE_INTEGRITY at binary offset 0x{offset:X} "
          f"(flash addr 0x{0x26000 + offset:08X})")

    # Struct layout: [magic_head:4][crc32:4][size:4][magic_tail:4]
    crc_offset = offset + 4
    size_offset = offset + 8

    # Write firmware size
    struct.pack_into('<I', data, size_offset, fw_size)

    # Zero out crc32+size (8 bytes) for CRC computation
    saved = bytes(data[crc_offset:crc_offset + 8])
    data[crc_offset:crc_offset + 8] = b'\x00' * 8
    crc = zlib.crc32(bytes(data)) & 0xFFFFFFFF
    data[crc_offset:crc_offset + 8] = saved

    # Write CRC32
    struct.pack_into('<I', data, crc_offset, crc)

    with open(output_path, 'wb') as f:
        f.write(data)

    print(f"CRC32:  0x{crc:08X}")
    print(f"Size:   {fw_size} bytes")
    print(f"Output: {output_path}")


if __name__ == '__main__':
    main()
