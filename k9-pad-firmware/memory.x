/* nRF52840 memory layout for Adafruit nRF52 Bootloader 0.10.0 + SoftDevice S140 v6.1.1
 *
 * Flash layout:
 *   0x00000000 - 0x00001000  MBR (Master Boot Record, 4 KB)
 *   0x00001000 - 0x00026000  SoftDevice S140 v6.1.1 (148 KB)
 *   0x00026000 - 0x000ED000  Application firmware (820 KB) ← FLASH region
 *   0x000ED000 - 0x000F3000  Reserved (24 KB)
 *   0x000F3000 - 0x000FC000  Bootloader (36 KB)
 *   0x000FC000 - 0x000FE000  Bootloader settings (8 KB)
 *   0x000FE000 - 0x000FF000  MBR params (4 KB)
 *   0x000FF000 - 0x00100000  Settings page — FlashStore KV (4 KB)
 *
 * RAM layout:
 *   0x20000000 - 0x20000008  SoftDevice RAM (reserved, 8 bytes minimum)
 *   0x20000008 - 0x2003FFFF  Application RAM (255 KB) ← RAM region
 *   Total: 256 KB
 */
MEMORY
{
  FLASH : ORIGIN = 0x00026000, LENGTH = 820K
  RAM : ORIGIN = 0x20000008, LENGTH = 255K
}