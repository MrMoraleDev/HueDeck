<!-- SPDX-License-Identifier: GPL-3.0-or-later -->

# ASUS Aura keyboards (ROG / TUF, `0B05`)

Plugin: `plugins/asus-aura-keyboard/` (`plugin.js`, `layouts.js`). Untested on hardware
(`tested` is empty).

Per-key lighting of the ASUS keyboards OpenRGB drives with two related protocols: its "Aura
keyboard" controller (the ROG Strix Scope TKL family) and its "Aura TUF keyboard" controller (ROG
Azoth, Claymore, Falchion, Strix Flare / Flare II / Flare II Animate, Strix Scope / Scope RX /
Scope NX Wireless Deluxe / Scope II / Scope II 96, TUF Gaming K1, K3, K3 Gen II, K5, K7). Both
send per-LED colours with `00 C0 81` reports; neither needs a mode switch for direct colours
(except the Claymore's remote-control command), and neither is ever sent a save.

## Sources

OpenRGB commit `d4798c120659ec5ba2e664a9e87bedff8d36f880`
(https://gitlab.com/CalcProgrammer1/OpenRGB). Every file below carries
`SPDX-License-Identifier: GPL-2.0-or-later`.

| File | License | Authors (header) | Taken |
|---|---|---|---|
| `Controllers/AsusAuraUSBController/AsusAuraUSBControllerDetect.cpp` | GPL-2.0-or-later | none named | Product ids, names, `REGISTER_HID_DETECTOR_IP` interface / usage page, which controller each id gets, `GetKeyboardMappingLayoutType` |
| `.../AsusAuraKeyboardController/AsusAuraKeyboardController.cpp`, `.h` | GPL-2.0-or-later | Adam Honse (CalcProgrammer1), 19 Mar 2020 | `SendDirect`: report layout, 15 LEDs per report, the count byte, the `0xFF` padding, the trailing read |
| `.../AsusAuraKeyboardController/RGBController_AsusAuraKeyboard.cpp`, `.h` | GPL-2.0-or-later | Adam Honse (CalcProgrammer1), 19 Mar 2020 | Scope TKL layout: `scope_tkl_matrix_map`, `default_tkl_led_names` (+ the two inserted keys), zones, frame buffer (`id R G B` per LED) |
| `.../AsusAuraTUFKeyboardController/AsusAuraTUFKeyboardController.cpp`, `.h` | GPL-2.0-or-later | Mola19, 03 Mar 2021 | `UpdateLeds`, `GetLayout`, `GetNumpadLocation`, `AllowRemoteControl`, `ClearResponses` / `AwaitResponse`, the per-LED flag (K1 / K5). `SaveMode`, `UpdateDevice` and the effect functions are read but not used |
| `.../AsusAuraTUFKeyboardController/RGBController_AsusAuraTUFKeyboard.cpp`, `.h` | GPL-2.0-or-later | Mola19, 03 Mar 2021 | Which layout table each product uses, layout selection and fallback, start sequence (`DeviceUpdateMode` with Direct) |
| `.../AsusAuraTUFKeyboardController/AsusAuraTUFKeyboardLayouts.h` | GPL-2.0-or-later | Mola19, 02 Nov 2021 | All TUF-protocol layouts: matrix maps, LED lists with LED ids (generated into `layouts.js`) |
| `RGBController/RGBControllerKeyNames.cpp` | GPL-2.0-or-later | Chris M (Dr_No), 25 Jan 2022 | Names of the Nordic keys (`KEY_NORD_*`), which HueDeck has no name of its own for |
| `DetectionManager.h` | GPL-2.0-or-later | | Detector macro semantics. Read only |

`layouts.js` is generated from those tables by a script in the porting session's scratch space
(not in the repository). HueDeck key names follow `docs/host-api/hid-usb.md` section 6, as in the
other keyboard plugins.

## Products

All `REGISTER_HID_DETECTOR_IP(..., interface, 0xFF00)`: HueDeck's Validate takes that interface
with usage page `0xFF00`.

| PID | OpenRGB name | Protocol | Interface | Layout table |
|---|---|---|---|---|
| `190C` | ASUS ROG Strix Scope TKL | Aura keyboard | 1 | Scope TKL |
| `1A05` | ASUS ROG Strix Scope RX TKL Wireless Deluxe | Aura keyboard | 1 | Scope TKL |
| `1954` | ASUS ROG Strix Scope TKL PNK LTD | Aura keyboard | 1 | Scope TKL |
| `19D0` | ASUS ROG Strix Scope NX TKL Moonlight White | Aura keyboard | 1 | Scope TKL |
| `1A83`, `1A85` | ASUS ROG Azoth USB / 2.4GHz | TUF | 1 | AsusROGAzothLayouts |
| `184D` | ASUS ROG Claymore | TUF | 1 | AsusClaymore{NoNumpad,NumpadRight,NumpadLeft}Layouts |
| `193C`, `193E` | ASUS ROG Falchion (Wired / Wireless) | TUF | 1 | AsusFalchionLayouts |
| `1875`, `18CF`, `18AF` | ASUS ROG Strix Flare / PNK LTD / CoD Black Ops 4 Edition | TUF | 1 | AsusROGStrixFlareLayouts |
| `19FC` | ASUS ROG Strix Flare II Animate | TUF | 1 | AsusROGStrixFlareIIAnimateLayouts |
| `19FE` | ASUS ROG Strix Flare II | TUF | 1 | AsusROGStrixFlareIILayouts |
| `18F8`, `1951`, `1B12`, `19F6`, `19F8` | ASUS ROG Strix Scope / RX / RX EVA-02 / NX Wireless Deluxe USB / 2.4GHz | TUF | 1 | AsusROGStrixScopeLayouts |
| `1AB3`, `1AB5` | ASUS ROG Strix Scope II / II RX | TUF | 1 | AsusROGStrixScopeIILayouts |
| `1AAE` | ASUS ROG Strix Scope II 96 Wireless USB | TUF | 1 | AsusROGStrixScopeII96WirelessLayouts |
| `1B78` | ASUS ROG Strix Scope II 96 RX Wireless USB | TUF | 1 | AsusROGStrixScopeII96RxWirelessLayouts |
| `1945` | ASUS TUF Gaming K1 | TUF (not per-LED) | 2 | AsusTufK1Layouts |
| `194B`, `18AA` | ASUS TUF Gaming K3 / K7 | TUF | 1 | AsusTUFK7Layouts |
| `1B30`, `1C5E` | ASUS TUF Gaming K3 GEN II / MIKU EDITION | TUF | 1 | AsusTUFK3GamingGen2Layouts |
| `1899` | ASUS TUF Gaming K5 | TUF (not per-LED) | 2 | AsusTufK1Layouts |

## Protocol

Every report is a 65-byte output report, report id `0x00` first. OpenRGB reads input reports
with hidapi, which strips the `0x00` report id on Windows; "data byte n" below is the n-th byte
after it (HueDeck's `device.read` keeps the report id, so data byte n is at index n + 1).

### Aura keyboard (Scope TKL)

- **Start**: nothing.
- **LEDs**: OpenRGB's LED order: the 86 keys (`default_tkl_led_names` with Left Windows `0x15`
  inserted at position 7 and Left Alt `0x1D` at 12), then zone "Logo" (2 LEDs, ids `0x80`,
  `0x90`), then zone "Underglow" (26 LEDs). HueDeck grid 26 x 8: the keys at their cells of
  OpenRGB's 6 x 18 matrix map, the logo on row 6, the underglow on row 7.
- **Every frame** (`SendDirect(114, frame)`): `00 C0 81 <n> 00` + up to 15 x `<id> R G B`;
  `<n>` starts at (number of reports x 15) = 120 and drops by 15 per report; the unused tail of
  the last report is `0xFF`. Then one input report is read ("Read to apply"; OpenRGB blocks
  until it arrives, HueDeck waits at most 20 ms).

### Aura TUF keyboards

Every command is preceded by `ClearResponses` (drain pending input reports; HueDeck:
`clearReadBuffer`) and, except the probes, followed by `AwaitResponse(20)` (read one input report,
20 ms timeout).

- **Start** (OpenRGB constructor + `SetupZones` + `DeviceUpdateMode` with Direct active):
  - all but the Claymore: `00 12 12` (`GetLayout`), read one report; layout number
    `= data[4] * 100 + data[5]`, truncated to 8 bits as OpenRGB stores it (`unsigned char`).
    If the table has no entry `layout % 100`, OpenRGB falls back to UK (4) when
    `layout / 100 == 2`, else US (17); if the table has no entry for that either (the K3 Gen II
    table has only US), OpenRGB ends up with an empty layout: HueDeck sends nothing then.
  - Claymore: `00 40 60` (`GetNumpadLocation`), read one report; numpad
    `= data[5] * 2 + data[4]`: 2 = right, 3 = left, else none, which picks the table. Its layout
    number comes from the USB release number (`bcdDevice >> 12`), which HueDeck's host does not
    expose: HueDeck uses OpenRGB's default branch, 117 (US). An ISO Claymore therefore gets the
    US LED list (its ISO keys stay dark). Then `00 41 01` and `00 41 03`
    (`AllowRemoteControl(1)`, `(3)`).
  - OpenRGB's blocking reads of the two probes are bounded to 1 s in HueDeck; no reply counts as
    layout 0 (US) / no numpad.
  - Left out: `GetVersion` (`00 12 00`, on the Claymore `00 10 01 00 00 00` + `00 12 22`); it
    only fills OpenRGB's device information.
- **LEDs**: the layout's LED list in order, each at its cell of the layout's matrix map. LED
  names are HueDeck key names (OpenRGB leaves these LEDs unnamed).
- **Every frame** (`UpdateLeds`): `00 C0 81 <count> 00` + `count` x `<id> R G B`, 15 LEDs per
  report. TUF Gaming K1 and K5 (not per-LED): the id byte is `00` for every LED.

HueDeck skips a frame whose reports did not change, resending them after 1 s.

**Shutdown.** OpenRGB has no exit step, and these keyboards have no plain "back to the onboard
effect" command in it (the Claymore's `AllowRemoteControl(0)` only follows a hardware-mode change
and a save). So HueDeck sets every LED black with a normal frame, on exit and on suspend. The
keyboard shows its saved effect again after it is reconnected.

## Safety: left out

- `SaveMode` (`00 50 55`): saves the current mode to the keyboard. Never sent.
- Hardware effects: `UpdateDevice` (`00 51 2C ...`), `UpdateMode` (`00 51 28`),
  `UpdateQuicksandColors` (`00 51 91`), the K1 wave and Scope II ripple / quicksand variants.
- `UpdateSingleLed` (same `00 C0 81` report with one LED): HueDeck always sends whole frames.
- Nothing in these protocols touches key mapping, macros, polling rate or other settings.

## Untested

Every product id. Open points for a first hardware test: whether the Scope TKL keyboards send an
input report after a frame (OpenRGB waits for one forever), the Claymore layout (bcdDevice), and
the descriptors (HueDeck assumes interface 1, or 2 on the K1 / K5, with usage page `0xFF00`, as
OpenRGB's detectors say).
