<!-- SPDX-License-Identifier: GPL-3.0-or-later -->

# MSI graphics cards: RGB controller at I2C 0x68

The plugin is `plugins/msi-gpu/`. It drives the RGB controller MSI puts on its graphics
cards, at 7-bit address `0x68` on the GPU's own I2C bus. On NVIDIA cards Lightloom reaches
that bus through NvAPI (`crates/lightloom-nvapi`), port 1, from its own (unelevated)
process.

**Status: tested on the reference card (2026-10-19).** `lightloom light red|green|blue|white`
and `lightloom rainbow` (then with `--smbus`) on the RTX 3060 Gaming X Trio LHR: the card showed each
colour and the rainbow (checked by eye). Solid colours: 19 writes per run, 0 errors. Rainbow
at 30 fps: 1811 writes in 15 s with 1 transient NvAPI error (status -1), recovered on the next
frame; a 4 s run had 0 errors. Register 0x3F was never written.

2026-10-22: after a run together with the RAM, the card came back to its startup effect
but darker. `Shutdown()` cleared register 0x26 instead of restoring it and did not check
the restore writes (one NvAPI write in a few hundred fails with status -1). Register 0x26
is now saved at `Initialize()` and restored, and each restored register is read back and
written once more if the card did not take it.

| Card | PCI vendor:device | Subsystem | Status |
|---|---|---|---|
| MSI GeForce RTX 3060 Gaming X Trio LHR | `10DE:2504` | `1462:3903` | tested 2026-10-19 |

## Sources

All OpenRGB files are at commit `d4798c12` (https://gitlab.com/CalcProgrammer1/OpenRGB),
each "part of the OpenRGB project" with `SPDX-License-Identifier: GPL-2.0-or-later` and
no author line (copyright: OpenRGB contributors, see the OpenRGB git history).

| # | Source | Used for |
|---|--------|----------|
| S1 | `Controllers/MSIGPUController/MSIGPUController/MSIGPUController.h` | Register map, mode values, brightness scale |
| S2 | `Controllers/MSIGPUController/MSIGPUController/MSIGPUController.cpp` | One SMBus Write Byte Data per register; Read Byte Data for reads; `Save()` = `0x3F ← 0x01` |
| S3 | `Controllers/MSIGPUController/MSIGPUController/RGBController_MSIGPU.cpp` | What is read at start, the "Direct" mode sequence, the per-frame sequence |
| S4 | `Controllers/MSIGPUController/MSIGPUController/MSIGPUControllerDetect.cpp` | Address 0x68, the card list (line 70 for the RTX 3060 Gaming X Trio LHR), NVIDIA buses on port 1 only |
| S5 | `pci_ids/pci_ids.h` | `NVIDIA_RTX3060_LHR_DEV = 0x2504`, `MSI_SUB_VEN = 0x1462`, `MSI_RTX3070_GAMING_X_TRIO_SUB_DEV = 0x3903` |
| S6 | `docs/research/m5-privileged-buses.md` §4 | NvAPI I2C (`I2CReadEx`/`I2CWriteEx`, `NV_I2C_INFO_V3`, port 1, never DDC) |

The plugin's code is adapted from S1-S4 (GPL-2.0-or-later, so it can be GPL-3.0-or-later
here); no SignalRGB code is used.

## Registers (S1)

| Register | Name | Lightloom |
|---|---|---|
| `0x22` | mode | read at start; `0x13` (static) on Initialize; restored last on Shutdown |
| `0x26` | "unknown" | written `0x00` before every colour update, as OpenRGB does |
| `0x27..0x29` | colour 2 (R, G, B) | read at start, restored on Shutdown |
| `0x2A..0x2C` | colour 3 (R, G, B) | read at start, restored on Shutdown |
| `0x30..0x32` | colour 1 (R, G, B) | the frame colour |
| `0x36` | brightness | `0x64` (5 × 20) on Initialize; restored on Shutdown |
| `0x38` | effect speed | read at start, restored on Shutdown |
| `0x3F` | save (write `0x01` stores the state in the card's flash) | only by "Save to devices" (`SaveToDevice`), see below |

Every transfer is an SMBus Write Byte Data or Read Byte Data to `0x68`. On NvAPI one
register byte plus one data byte (`NV_I2C_INFO_V3`, `i2cDevAddress = 0xD0`, port 1,
`bIsDDCPort = 0`).

## Sequences

All of them are in `plugins/msi-gpu/fixtures/expected.json`, worked out by hand from S3.

- **Scan:** an id match on an NVIDIA bus (all four PCI ids) returns `[0x68]`. No I/O, as
  OpenRGB's detector, which does not probe the card either.
- **Initialize:** read `0x30 0x31 0x32 0x27 0x28 0x29 0x2A 0x2B 0x2C` (S3 `SetupZones()`),
  `0x22` (`GetDeviceMode()`), `0x38` (`GetSpeed()`), `0x36` (brightness); then "Direct"
  (`DeviceUpdateMode()`): `0x36 ← 0x64`, `0x22 ← 0x13`. Direct has no speed flag, so no
  speed write.
- **Frame** (`DeviceUpdateLEDs()`, only when the colour changed): `0x26 ← 0x00`,
  `0x30 ← R`, `0x31 ← G`, `0x32 ← B`. OpenRGB limits updates to one per 33 ms; Lightloom's
  host renders SMBus devices at most every 33 ms too, and the plugin sends nothing for an
  unchanged colour.
- **Shutdown:** if every start-up read succeeded: `0x26 ← 0x00`, the nine colour registers,
  speed, brightness, then the mode last (it applies the others). If a read failed, the
  `shutdownColor` parameter (default black) in static mode: `0x26 ← 0`, colour 1, `0x22 ← 0x13`.

One write per register and no retries (OpenRGB's behaviour); a failed write is logged once.

## Bus and safety

- The bus is offered only to plugins whose `BrandGPUList()` names the card; the plugin's
  `SmbusAddresses()` is `[0x68]`, so its scan and its device session can reach no other
  address (`crates/lightloom-plugin-host/src/smbus.rs`).
- Register `0x3F` is refused by the host (`SmbusDeniedRegisters()`) at all times except
  inside the `SaveToDevice({action: "save"})` call the user asked for, so no other code path
  of the plugin and no other plugin bound to `0x68` can store state in the card's flash.

## Save to devices and exit modes (Lightloom extensions, untested on the card)

"Save to devices" ([docs/host-api/save-shutdown.md](../host-api/save-shutdown.md)) stores one of
three of OpenRGB's modes in the card's flash, with OpenRGB's own sequence: `DeviceUpdateMode()`
(brightness `0x36 ← 0x64`, then the speed `0x38 ← 0x02` = `speed_values[MSI_GPU_SPEED_MID]` for
modes that have one, then the mode `0x22`), `DeviceUpdateLEDs()` for modes with a colour
(`0x26 ← 0`, colour 1), and `DeviceSaveMode()` = `MSIGPUController::Save()` (`0x3F ← 0x01`) (S2, S3):

| Choice | Mode | Writes (fixture) |
|---|---|---|
| current colour | Static `0x13` | `save_static_123456` |
| `spectrum` | Rainbow `0x08` (no colour) | `save_spectrum` |
| `breathing` | Breathing `0x04`, colour 1 | `save_breathing_123456` |

The card then goes back to Direct (`save_resume` = the Initialize writes) for the live effect, and
the plugin's startup image takes the stored values, so a later restore (`shutdown_after_save`)
gives back what was stored rather than the state before it. OpenRGB writes `0x3F ← 0x01`; the
v2 controller (`MSIGPUv2Controller`, other cards) writes `0x00` to the same register, which this
plugin does not drive.

Exit modes (`Shutdown(false, mode)`): `restore` as above; `keep` writes the last frame colour in
static mode (`shutdown_keep_123456`); `off` static black (`shutdown_off`). Neither touches `0x3F`.
- Every NvAPI transfer goes to the card by PCI bus and ids, re-resolved after a driver
  reset, on one worker thread. It does not take `Global\Access_SMBUS.HTP.Method` (neither
  do OpenRGB and SignalRGB): with Lightloom's RAM sessions running, waiting for that mutex
  cut the card to 13 frames in 4 s with `NVAPI_DEVICE_BUSY` (-108) on 2026-10-19
  (ADR-0004, 2026-10-22).
- Lightloom refuses to touch SMBus/GPU I2C while SignalRGB, OpenRGB or a vendor lighting
  service (MSI `LEDKeeper*` / `Mystic_Light_Service`, ...) runs, with no override, and stops
  the GPU session when one starts (checked every second).
- NvAPI statuses reach the plugin unchanged as negative values (e.g. `-105`
  `NVAPI_PORTID_NOT_FOUND`); NvAPI has no NACK status.

## Hardware modes (Lightloom extension, untested on the card)

`HardwareModes()` offers the modes of OpenRGB's `RGBController_MSIGPU` except "Direct" (that is the
canvas) and "Off" (OpenRGB's `DeviceUpdateMode()` writes nothing for a mode without a brightness):
Static, Rainbow, Raindrop, Magic, Patrolling, Streaming, Lightning, Wave, Meteor, Stack, Rhythm,
Flowing, Whirling, Twisting, Laminating, Fade in, Breathing, Flashing and Double flashing, with the
`MSI_GPU_MODE_*` values of S1. `SetHardwareMode(request)` writes, as OpenRGB switches modes:

1. `0x36` brightness: `round(brightness% / 20)` (0-5) × 20;
2. `0x38` speed, for every mode but Static: `speed_values[round(speed% / 50)]` = `0x04` / `0x02` / `0x01`;
3. `0x22` the mode;
4. for modes with a colour: `0x26 ← 0`, then R1/G1/B1 (`0x30-0x32`); Fade in: R2/G2/B2 (`0x27-0x29`) and
   R3/G3/B3 (`0x2A-0x2C`), as `DeviceUpdateLEDs()` does.

Never `0x3F`: the mode is gone at the next power cycle. `SetHardwareMode(null)` writes Direct again
(`0x36 ← 100`, `0x22 ← 0x13`) and the next frame is sent whole. `Shutdown(false, "keep")` leaves the
mode running. Tests: `plugins/msi-gpu/tests/packets.test.mjs`, `crates/lightloom-plugin-host/tests/session_hwmode.rs`.
The card takes one colour in Direct mode; its moving effects come only from these modes.

## Per-LED mode (experimental, UNVERIFIED, off by default)

The "Per-LED (experimental)" parameter (`perLed`, default `false`) switches the plugin to the
per-LED stream MSI's LEDKeeper2 sends to non-"NV5000" NVIDIA cards, found by static analysis
of MSI's installed files ([research note](../research/msi-gpu-perled.md)). **Nobody has seen it
work on a card yet**: whether this card honours it, the colour order and which slot is which
LED are unknown. Nothing changes for users who leave the parameter off.

When on:

- **Initialize:** the usual startup reads (kept for the restore), then two read-only
  identification reads, `0xEC` (4 bytes: LED type, module type, firmware) and `0xED` (2 bytes:
  chip id). An "NV5000" controller (module type `0x5000` or LED type `0x81`) or a failed read
  keeps the one-colour path. Otherwise the device becomes a 34 x 1 strip ("LED 1".."LED 34",
  the mapping to physical LEDs is unknown) and the plugin writes `0x2E ← 0x01`. No brightness
  or mode write.
- **Render:** one raw I2C write (`WriteBlockWithoutRegister`, 137 bytes): `0x49`, then for
  each of the 34 slots `[B, G, R, 0x00]` sampled along the strip (MSI reverses the colour
  bytes; B, G, R is the best guess). Unchanged frames are resent every 60 frames.
- **Shutdown:** `0x2E ← 0x00` first, then the exit mode as usual (restore, keep or off).
- Hardware modes and "Save to devices" leave the stream first (`0x2E ← 0`) and enter it again
  afterwards; switching the parameter off while running leaves it and goes back to Direct.
- Until Shutdown only `0x2E` and `0x49` are written; `0x3F` stays refused by the host.

The host allows register-less writes of up to 140 bytes on NVIDIA buses for this
([host API](../host-api/smbus.md)); NvAPI gets `0x49` as the register and 136 data bytes, the
same call shape MSI's `NDA.dll` uses.

Before turning it on for anyone, run the supervised probe below on the reference card.

### Hardware probe: `lightloom probe msi-gpu-perled`

Stop lightloomd, SignalRGB and MSI Center's lighting service first (the probe refuses to run
beside them, as `light` does). Then, watching the card:

```
lightloom probe msi-gpu-perled --dry-run --auto 0   # prints every transfer, no NvAPI
lightloom probe msi-gpu-perled                      # step through the slots with Enter
lightloom probe msi-gpu-perled --order rgb          # if the colour check showed R and B swapped
```

It accepts only `10DE:2504 1462:3903`. It reads and prints `0xEC`/`0xED` and stops (nothing
written) on a failed read or an NV5000 controller; writes `0x2E ← 1`; lights each of the 34
slots alone in red (`--brightness`, default `0x40`; `--order bgr|rgb`, default `bgr`), waiting
for Enter (`q` stops) or `--auto <ms>`; shows all slots red for 3 s, then blue for 3 s; sends a
short frame (6 slots, 24 bytes, green) and prints its result code; and always ends with
`0x2E ← 0`: after the last step, on an error, on Ctrl-C or console close, and on a panic.
Every write is printed with its bytes. It can write nothing but `0x2E ← 0/1` and `0x49`
frames.

What to note: the identification values; for each slot, which LED lit (or none) and in which
colour; whether "all red" looked red (else rerun with `--order rgb`); whether the short frame
left slots 6.. blue (short frames accepted) or did nothing; what the card shows after the
probe ends.

## Trying it (with the card)

SMBus devices are on by default (`--no-smbus` leaves them out):

```
lightloom plugins                              # bus list and scan result (SignalRGB closed)
lightloom light red --device smbus:10de --dry-run
lightloom light red --device smbus:10de
```

`bench --mock-smbus msi-gpu` runs the plugin end to end on a scripted bus (no card needed).
