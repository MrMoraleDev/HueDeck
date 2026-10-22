<!-- SPDX-License-Identifier: GPL-3.0-or-later -->

# Crucial Ballistix RAM: RGB controller on the chipset SMBus

The plugin is `plugins/crucial-ballistix/`. It drives the RGB controller of Crucial
Ballistix DDR4 RGB modules (8 LEDs per module) on the chipset SMBus: the AMD FCH
("PIIX4", port 0x0B00) or one of the Intel SMBus controllers OpenRGB lists for DRAM. On
Windows Lightloom reaches that bus only through the Lightloom I/O broker
(`crates/lightloom-iobroker`, a LocalSystem service) and the PawnIO driver; Lightloom's own
processes never open PawnIO.

**Status: tested on the reference machine (2026-10-22, build ef2a473).** Both modules and
the RTX 3060 together: `light red|green|blue|white` (2 s each) and `rainbow` (4 s), every
colour seen by the user on both modules. `Initialize()` took 143-150 ms per module (was
about 3 s on 2026-10-19 before batching); rainbow ran 111 frames per module in 4 s with
0 errors and 0 frames skipped for a busy bus. The SPD check read both slots (0x52, 0x53)
as DDR4, JEDEC bank 6 id 0x9B (Crucial Technology), part BL8G32C16U4WL.8FE. After a
reboot the modules answer at 0x27 again and a live scan remaps them; `--dry-run` reports
that without writing. (The schema keeps `tested` for PCI tuples, so RAM runs are recorded
here.)

| Module | Bus on the reference machine | Addresses seen | Status |
|---|---|---|---|
| 2 x Crucial Ballistix BL8G32C16U4WL | `AMD PIIX4 Bus - 0x0b00` (`1022:790B`, MSI MAG B550 Tomahawk) | `0x39`, `0x3A` (SignalRGB log, research §1.7); `0x3B`, `0x3C` after Lightloom's remap (2026-10-19) | tested 2026-10-22, both modules |

## What the user needs

1. The PawnIO driver, official edition, 2.2.0 or later (https://pawnio.eu). Installing it
   is the user's step for now (an option of the Lightloom installer and an install / remove
   button in Settings are planned). Without it `lightloom` and the daemon say that RAM
   lighting is off (from PawnIO's uninstall entry) and GPU lighting keeps working.
2. Once, from an elevated prompt: `lightloom-setup --smbus --pawnio-modules <dir>`, with
   `<dir>` the PawnIO.Modules 0.2.11 release as downloaded (`SmbusPIIX4.bin`, `Echo.bin`,
   `COPYING`; SHA-256 pinned). It registers the `LightloomIoBroker` service. A broker of an
   older protocol (before v3) is reported as such and not used: run it again to update.
3. SignalRGB and OpenRGB closed: Lightloom never touches the SMBus alongside them.

Then RAM lighting is on by default (`lightloom light red`, `lightloom plugins`, the daemon);
`--no-smbus` or `[smbus] enabled = false` turns SMBus off. A `lightloom` run while `lightloomd`
runs leaves the SMBus devices to the daemon.

## Sources

OpenRGB files at commit `d4798c12` (https://gitlab.com/CalcProgrammer1/OpenRGB), each
"part of the OpenRGB project" with `SPDX-License-Identifier: GPL-2.0-or-later`; the
controller files carry "Adam Honse (CalcProgrammer1) 19 Jan 2020".

| # | Source | Used for |
|---|--------|----------|
| S1 | `Controllers/CrucialController/CrucialController.h` | Register map (version 0x1000, "Micron" at 0x1025 / 0x1030), mode values (static 0xCF) |
| S2 | `Controllers/CrucialController/CrucialController.cpp` | Indexed register access, effect mode, effect colours, the three direct colour planes, the byte fallback |
| S3 | `Controllers/CrucialController/CrucialControllerDetect.cpp` | Address list, the remap of modules at 0x27, detection (`TestForCrucialController`) |
| S4 | `Controllers/CrucialController/RGBController_Crucial.cpp` | Zone (8 LEDs, "DRAM LED n"), "Direct" mode sequence, what a frame sends |
| S5 | `pci_ids/pci_ids.h` | `IF_DRAM_SMBUS`: the chipset SMBus controllers scanned for DRAM |
| S6 | `docs/research/m5-privileged-buses.md` §1, §6 | PawnIO, `SmbusPIIX4` transfers, the global SMBus mutex, the broker |

The plugin's code is adapted from S1-S5 (GPL-2.0-or-later, so it can be GPL-3.0-or-later
here); no SignalRGB code is used.

## Protocol (S1, S2)

The controller has a 16-bit indexed register file behind three SMBus commands:

| Transfer | Meaning |
|---|---|
| Write Word, command `0x00`, value `((reg << 8) & 0xFF00) \| (reg >> 8)` | set the register pointer (byte-swapped: the register's high byte goes out first) |
| Read Byte, command `0x81` | read the pointed register |
| Write Byte, command `0x01` | write the pointed register |
| SMBus Block Write, command `0x03`, count `n`, `n` bytes | write `n` registers from the pointer |

| Register | Name | Lightloom |
|---|---|---|
| `0x1000..0x100F` | version string | read at Initialize, shown in the device name |
| `0x1025` / `0x1030` | "Micron" | detection |
| `0x820F` | mode | `0xCF` (static) |
| `0x82E9` | effect LED mask (`1 << led`) | effect colours |
| `0x82EA..0x82EC` | effect arguments | `0x00` |
| `0x82ED`, `0x82EE`, `0x82EF` | effect red, green, blue; also mode arguments (`0x82EE` = 0, `0x82EF` = speed 0x10) and remap arguments | |
| `0x82F0` | apply: `0x01` colour, `0x84` mode, `0xF0` remap | |
| `0x8300..0x8307` | direct red, LEDs 0-7 | frame |
| `0x8340..0x8347` | direct green | frame |
| `0x8380..0x8387` | direct blue | frame |

There is no save/EEPROM register: nothing persists past a power cycle, and Lightloom writes
nothing else. The SPD EEPROM (0x50-0x57) and its write-protect addresses (0x30-0x37) are
never addressed; the plugin host and the broker both refuse writes there anyway.

## SPD check (which modules are there)

Before the plugin's `Scan(bus)` runs, the host reads the SPD of the DIMM slots
(0x50-0x57) and offers the scan only when a module's manufacturer is one of the plugin's
`SpdManufacturers()` (docs/host-api/smbus.md). The plugin declares both JEP106 ids a
Ballistix module may carry in SPD bytes 320-321:

| JEP106 | Bytes 320-321 as stored | Why |
|---|---|---|
| Crucial Technology, bank 6, id 0x9B | `0x85 0x9B` | Crucial's own module id |
| Micron Technology, bank 1, id 0x2C | `0x80 0x2C` | Micron (Crucial's parent) module id, also seen on Crucial-branded modules |

The SPD of the reference modules has not been read yet: SignalRGB's log shows "Manufacturer:
Unknown, Part Number: BL8G32C16U4WL.8FE" (from WMI). `lightloom plugins --dry-run` prints the
SPD of each slot it matched (`SPD check passed: 0x50 DDR4 bank 6 id 0x9b ...`) or, when
none matched, every module found; that settles which id these modules carry. Only DDR4 is
handled; on a DDR5 system RAM plugins do not scan.

## Timing

On 2026-10-19 one module's `Initialize()` took about 3 s: about 190 broker requests (168
transfers, each its own request, plus 20 lock / unlock requests), and each request waited
at least one Windows timer tick (15.6 ms): the PIIX4 module polled in ShortBusy mode, which
sleeps with `KeDelayExecutionThread`, and the broker slept 1 ms between requests under a
lease. Runs of 2 s therefore never reached `Render()`. Now:

- the broker polls the PIIX4 in AlwaysBusy mode (`KeStallExecutionProcessor`, no timer
  tick) and only yields, never sleeps, for the first 2 ms between requests under a lease;
- the plugin declares `SmbusBatchWrites()`: inside a `TransactionLock` its writes are
  queued and sent as one broker `Batch` (one round trip, one hold of the global mutex) at
  the next read or at `TransactionUnlock()`.

`Initialize()` becomes 45 requests for 168 transfers, at most about 240 ms even with
0.5 ms per round trip and 1.3 ms per transfer, and nearer 90 ms at the real bus speed; a
frame is 3 requests (lock, one batch of 6 transfers, unlock), a few milliseconds
(`crates/lightloom-plugin-host/tests/session_smbus.rs`,
`crucial_initialize_and_frame_fit_the_time_budget`). The session report prints
`Initialize took ... ms`, to be confirmed on the reference machine.

## Two modules

Each module is its own session on the same bus. The sessions take turns (one
first-come-first-served queue per bus): a session waits at most 100 ms for the other's
locked sequence, then skips that frame (`contended`, not an error) and sends the next one.
On 2026-10-19 the second module's `TransactionLock` was refused because the GPU's NvAPI
transfers held the global SMBus mutex too; they no longer take it (ADR-0004, 2026-10-22).

## Sequences

All of them are in `plugins/crucial-ballistix/fixtures/expected.json`, worked out from S2-S4.

- **Scan** (S3), only on a chipset bus whose controller is in `IF_DRAM_SMBUS` (S5):
  1. Remap, up to 4 slots: while a module answers Receive Byte at `0x27`, find the next
     address of `0x39 0x3A 0x3B 0x3C 0x20 0x21 0x22 0x23` that does not answer (the list
     position carries over between slots) and send it the module at `0x27`:
     `0x82EE ← slot`, `0x82EF ← address << 1`, `0x82F0 ← 0xF0`; 1 ms pause. The new
     address lasts until the next power cycle. **Lightloom differs:** before writing to
     `0x27` it reads registers `0xA0..0xAF` there and remaps only when they read
     `0x00..0x0F` (the controller's identification, as in step 2); anything else at `0x27`
     is left alone and the remap stops. OpenRGB writes to whatever answers.
  2. Detection at each of the 8 addresses: Receive Byte answers, registers `0xA0..0xAF`
     (plain Read Byte) read `0x00..0x0F`, and 16 registers from `0x1025` (else `0x1030`)
     read "Micron" and a NUL; 1 ms pause between addresses.
  3. **Dry run** (`--dry-run`, `bus.IsDryRun()`): writes are recorded, never sent, and
     reads are real. A module still at `0x27` is reported ("needs an address: a live scan
     moves it ... to 0x39") and not remapped; a module already given an address (after a
     live scan, until the next power cycle) is found on the identification registers
     alone, since the "Micron" check needs a pointer write. The scan's log lines are
     printed with its outcome.
- **Initialize** ("Direct", `DeviceUpdateMode()`): version string read; mode
  `0x820F ← 0xCF`, `0x82EE ← 0x00`, `0x82EF ← 0x10`, `0x82F0 ← 0x84`; then each LED's
  current colour as an effect colour (`0x82E9 ← 1 << led`, `0x82EA..EC ← 0`,
  `0x82ED/EE/EF ← r/g/b`, `0x82F0 ← 0x01`).
- **Frame** (`DeviceUpdateLEDs()` in Direct, only when a colour changed): pointer `0x8300`
  then a Block Write of the 8 red values to `0x03`; the same for green at `0x8340` and blue
  at `0x8380`. Six transfers. If a Block Write fails, the 8 bytes go one by one to `0x01`
  (S2's fallback). Lightloom holds `TransactionLock` for the whole frame: through the broker
  that keeps the global SMBus mutex (a 100 ms `BusLock` lease) so no other program's
  transfer lands between a pointer write and its data. When the bus cannot be held the
  frame is not sent and goes out with the next one. The six writes go out as one batch;
  when it fails (`TransactionUnlock()` answers a negative status) the planes go out again
  byte by byte under a new lock.
- **Every register access** (pointer write plus its data transfer), in Scan, Initialize and
  Shutdown too, runs under `TransactionLock` (sequences such as a mode change or one LED's
  effect colour under one hold), so neither another session's scan nor another program
  moves the pointer in between; when the bus cannot be held nothing is sent. The host also
  stops the RAM sessions before a rescan.
- **Shutdown**: the shutdown colour (parameter `shutdownColor`, default black; black when
  the system suspends) as effect colours on all 8 LEDs, then static mode (S4's
  `DeviceUpdateLEDs()` order for "Static").

## Bus and safety

- The plugin's `SmbusAddresses()` is `0x20-0x23, 0x27, 0x39-0x3C`: its scan reaches nothing
  else, and each device session only its own address, which the scan must have found
  (`crates/lightloom-plugin-host/src/smbus.rs`).
- The broker serves chipset kinds only, writes only to `0x20-0x23, 0x27, 0x39-0x3C` (its
  own allowlist), refuses writes to the SPD, PMIC and EEPROM ranges, block reads and
  register-less block transfers, limits blocks to 32 bytes and 2000 operations per
  second, takes `Global\Access_SMBUS.HTP.Method` around every transfer, batch or lease
  (50 ms bounded wait), checks every op of a batch as if it came alone, and re-selects
  PIIX4 port 0 at each hold (putting back a mux another program left on port 2-4).
- The SPD is only read: the host's SPD check goes through the broker's `SpdRead`, whose
  EE1004 page select (0x36 / 0x37) is the broker's own step under the mutex, with page 0
  put back; no plugin can reach it.
- Failures reach the plugin as `-1` (SignalRGB's chipset convention).
- Lightloom refuses to touch the SMBus while SignalRGB, OpenRGB or a vendor lighting service
  runs, with no override, and stops the RAM sessions when one starts (checked every second).
- Frames go out at most every 33 ms.
