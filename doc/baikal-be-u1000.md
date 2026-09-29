# Baikal BE-U1000 / EVU-BA-2.5

The built-in `BE-U1000` target currently supports Core 0 through the EVU-BA-2.5
on-board FT2232H USB-JTAG adapter. Core 1 and Core 2 use separate TAPs and are
not exposed as probe-rs cores yet.

## EVU-BA-2.5 adapter profile

Select the board profile:

```console
probe-rs read b32 0x40010000 4 --board evu-ba-2.5
```

The profile selects the `BE-U1000` target and FT2232H channel B
(`0403:6010-1`), configures the board's nTRST/nSRST pins and falling-edge TDO
sampling, and enables physical target reset. Its default JTAG speed request is
8 MHz; the FT2232H divisor produces an actual 7.5 MHz clock. Explicit
`--chip`, `--probe`, `--protocol`, and `--speed` arguments take precedence.

The profile does not silently choose between multiple matching adapters. Use
an explicit channel-B selector such as `--probe 0403:6010-1:<serial>` if more
than one is connected.

The tested EVU-BA-2.5 setup uses the JTAG EXT strap configuration on XP1
(`DBG-M2-M1-M0=0100`, only M2 installed) and the on-board USB-JTAG connector
XS2.

## Main eFlash

Flash programming is selected automatically from the image load addresses. An
ELF whose loadable segments target the main eFlash can be downloaded directly:

```console
probe-rs download firmware.elf --board evu-ba-2.5 --verify --start
```

The host-side flash sequence exposes only the 256 KiB main array at
`0xa0000000..0xa0040000`. It protects NVR, issues the main-array-only erase
command, programs aligned words through the eFlash controller, and verifies the
programmed bytes. The direct-programming aperture remains disabled throughout;
controller commands never select NVR. Each download currently erases and
replaces the complete main array; it cannot preserve unrelated data already in
that array. Images linked into TCMA are written only to RAM and never invoke the
eFlash sequence. NVR protection is sticky until an external reset, so the EVU
board profile should remain enabled while flashing.

EVU-BA-2.5 is normally strapped for JTAG EXT boot. In that mode the BootROM
waits for a debugger after reset instead of booting eFlash. For ELF downloads,
probe-rs therefore records the ELF entry point and `--start` resumes directly
from it after reset. A successful direct start does not prove that an image is
BootROM-compatible. Firmware intended for standalone autoboot must also contain
the Baikal boot header and memory-specific magic and the board must use the
corresponding boot straps.

## GDB

Start the probe-rs GDB server with the same board profile:

```console
probe-rs gdb firmware.elf --board evu-ba-2.5
```

Core 0 supports halt/resume, register and memory access, single-step, and four
hardware triggers.
