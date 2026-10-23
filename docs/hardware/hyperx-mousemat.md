<!-- SPDX-License-Identifier: GPL-3.0-or-later -->

# HyperX RGB mousemats: Fury Ultra, Pulsefire Mat, Pulsefire Mat RGB Mouse Pad XL

Plugins: `plugins/hyperx-mousemat/` (Kingston vendor id `0951`) and `plugins/hyperx-mousemat-hp/`
(HP vendor id `03F0`, notes: [hyperx-mousemat-hp.md](hyperx-mousemat-hp.md)); `family.js` is
shared byte for byte (a test checks it). Untested on hardware (`tested` is empty).

## Sources

OpenRGB commit `d4798c120659ec5ba2e664a9e87bedff8d36f880` (https://gitlab.com/CalcProgrammer1/OpenRGB).

| File (under `Controllers/HyperXMousematController/`) | License | Author in header | Taken |
|---|---|---|---|
| `HyperXMousematControllerDetect.cpp` | GPL-2.0-or-later | Adam Honse (CalcProgrammer1), 25 Oct 2020 | Product ids, names, interface / usage page / usage (the `_WIN32` branch for the XL pad), LED counts per product |
| `HyperXMousematController.cpp`, `.h` | GPL-2.0-or-later | same | `SendDirect` |
| `RGBController_HyperXMousemat.cpp`, `.h` | GPL-2.0-or-later | same | Zones, LED names, keepalive |

## Products

| Product id | OpenRGB name | LEDs | Validate |
|---|---|---|---|
| `0951:1705` | HyperX Fury Ultra | 15 underglow + 5 strip | interface 0, any collection |
| `0951:1741` | HyperX Pulsefire Mat RGB Mouse Pad XL | 2 underglow | interface 1, usage page `FF90`, usage `FF00` |
| `03F0:0F8D` | HyperX Pulsefire Mat | 15 underglow + 5 strip | interface 1, usage page `FF90`, usage `FF00` |

LEDs: "Underglow LED n" on row 0, "LED Strip LED n" on row 1 (OpenRGB's zone order).

## Protocol (Direct mode only)

Nothing at `Initialize()`. Per frame (`SendDirect`), three 65-byte feature reports: `00 04 F2 …
[9]=02`, then two reports of 16 groups `81 R G B` (colours 0-15, then 16-31). OpenRGB reads 32
colours whatever the LED count, past the end of its colour vector for 20 or 2 LEDs; Lightloom sends
those slots black. OpenRGB's keepalive resends after 50 ms without an update, so Lightloom sends a
frame on every `Render()`.

**Shutdown.** OpenRGB has no exit step and no "back to the stored lighting" command: one black
frame.

## Safety

Only these Direct reports exist in the source; nothing else is sent (a test checks every report
header).

## Untested

Every product id. The NGENUITY process names are a best guess.
