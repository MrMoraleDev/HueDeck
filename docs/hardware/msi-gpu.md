<!-- SPDX-License-Identifier: GPL-3.0-or-later -->

# MSI graphics cards: RGB controller at I2C 0x68

The plugin is `plugins/msi-gpu/`. It drives the RGB controller MSI puts on its graphics
cards, at 7-bit address `0x68` on the GPU's own I2C bus. On NVIDIA cards HueDeck reaches
that bus through NvAPI (`crates/huedeck-nvapi`), port 1, from its own (unelevated)
process.

**Status: untested on hardware.** The plugin and HueDeck's NvAPI path are proven
against mock buses only. `tested` in `manifest.json` stays empty until someone runs it.

| Card | PCI vendor:device | Subsystem | Status |
|---|---|---|---|
| MSI GeForce RTX 3060 Gaming X Trio LHR | `10DE:2504` | `1462:3903` | untested (reference machine) |

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

| Register | Name | HueDeck |
|---|---|---|
| `0x22` | mode | read at start; `0x13` (static) on Initialize; restored last on Shutdown |
| `0x26` | "unknown" | written `0x00` before every colour update, as OpenRGB does |
| `0x27..0x29` | colour 2 (R, G, B) | read at start, restored on Shutdown |
| `0x2A..0x2C` | colour 3 (R, G, B) | read at start, restored on Shutdown |
| `0x30..0x32` | colour 1 (R, G, B) | the frame colour |
| `0x36` | brightness | `0x64` (5 × 20) on Initialize; restored on Shutdown |
| `0x38` | effect speed | read at start, restored on Shutdown |
| `0x3F` | save (write `0x01` stores the state in the card's flash) | **never written** |

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
  `0x30 ← R`, `0x31 ← G`, `0x32 ← B`. OpenRGB limits updates to one per 33 ms; HueDeck's
  host renders SMBus devices at most every 33 ms too, and the plugin sends nothing for an
  unchanged colour.
- **Shutdown:** if every start-up read succeeded: `0x26 ← 0x00`, the nine colour registers,
  speed, brightness, then the mode last (it applies the others). If a read failed, the
  `shutdownColor` parameter (default black) in static mode: `0x26 ← 0`, colour 1, `0x22 ← 0x13`.

One write per register and no retries (OpenRGB's behaviour); a failed write is logged once.

## Bus and safety

- The bus is offered only to plugins whose `BrandGPUList()` names the card; the plugin's
  `SmbusAddresses()` is `[0x68]`, so its scan and its device session can reach no other
  address (`crates/huedeck-plugin-host/src/smbus.rs`).
- Register `0x3F` is refused by the host as well (`SmbusDeniedRegisters()`), so no edit of
  the plugin and no other plugin bound to `0x68` can store state in the card's flash.
- Every NvAPI transfer runs under `Global\Access_SMBUS.HTP.Method` and goes to the card by
  PCI bus and ids, re-resolved after a driver reset (ADR-0004).
- HueDeck refuses to touch SMBus/GPU I2C while SignalRGB, OpenRGB or a vendor lighting
  service (MSI `LEDKeeper*` / `Mystic_Light_Service`, ...) runs, with no override, and stops
  the GPU session when one starts (checked every second).
- NvAPI statuses reach the plugin unchanged as negative values (e.g. `-105`
  `NVAPI_PORTID_NOT_FOUND`); NvAPI has no NACK status.

## Trying it (later, with the card)

```
huedeck plugins --smbus                      # bus list and scan result (SignalRGB closed)
huedeck light red --smbus --device smbus:10de --dry-run
huedeck light red --smbus --device smbus:10de
```

`bench --mock-smbus msi-gpu` runs the plugin end to end on a scripted bus (no card needed).
