<!-- SPDX-License-Identifier: Apache-2.0 OR MIT -->

# Imported SignalRGB plugins on the reference machine (M2)

Unmodified SignalRGB 2.5.80 plugins, read in place from the user's own installation
(`lightloom ... --signalrgb`; nothing copied into this repository), driving the reference
machine's devices through Lightloom. Bench of 2026-10-18 with SignalRGB closed
(`taskkill /PID`, relaunched afterwards), release build, native HID backend.

Procedure per device: `lightloom light <colour> --signalrgb --device <vid:pid> --dry-run`
first (packets reviewed, the restore check where the device has feature reports), then a
live `lightloom rainbow --signalrgb --device <vid:pid> --duration N`.

## Results

| Device | Plugin (SignalRGB CDN cache) | Mode | Frames | Render p50 / p99 | HID errors | Notes |
|--------|------------------------------|------|--------|------------------|-----------|-------|
| MSI MAG B550 Tomahawk `1462:7c91` | `Msi/185 Byte Motherboard Controller/MSI_185Byte_Controller.js` | live 15 s | 426 | 13.0 / 13.0 ms | 0 | Subdevices `12v RGB Header 1/2` (3x3), channels `JRainbow 1/2` with SignalRGB's `Default Strip - 60` imported from the registry; restore check 0x52/0x53 identical. Render time is the 725-byte feature report (about 12 ms over USB full speed). |
| Razer DeathAdder V2 Pro `1532:007c` | `Razer/Razer_Modern_Mouse.js` | live 12 s | 109 | 1.97 / 2.79 ms | 0 | Mouse collection opened for feature reports only (Windows owns it). The first `Render()` sets the mouse up lazily: about 8 s (nine transaction ids probed 400 ms apart, then the configuration reads). `settingControl` defaults to false: no DPI or polling-rate change. |
| SteelSeries Arctis 5 `1038:12aa` | `Steelseries/Steelseries_Arctis_5_Headset_Alt_PID.js` | live 8 s | 232 | 22.0 / 23.0 ms | 0 | About 12 output reports per frame (2 ms each). |
| NZXT Kraken X3 `1e71:2007` | `Nzxt/Nzxt_Kraken_X3_AIO.js` | live 8 s | 232 | 13.0 / 14.0 ms | 0 | `Channel 1` with SignalRGB's `Default Strip - 20` imported. Only `0x22` lighting reports: pump and fan duty (`0x72`) are gated by `fanControlDisabled()`, which Lightloom answers `true`. |
| Sony DualSense `054c:0ce6` (USB) | `Sony/Sony_Dual_Sense_Controller.js` | dry-run 4 s | 112 | 0.06 / 0.10 ms | 0 | Not run live: the output report also sets headphone/speaker/microphone volume fields, outside a lighting test. |
| Sonix keyboard `320f:5064` | none | — | — | — | — | No SignalRGB plugin; needs a first-party plugin. |

## What the bench changed in Lightloom

- **System mice/keyboards** (Razer): Windows opens mouse and keyboard top-level collections
  exclusively; the native backend now falls back to a zero-access handle for feature reports,
  as hidapi does.
- **Watchdog**: `Render()` budgets count JavaScript time only (time in `device.pause()` and HID
  I/O excluded) under a 30 s wall-clock cap, so lazy device setup from `Render()` completes;
  a stop request still interrupts it at once.
- **Modules**: the SignalRGB MSI plugin needs `@SignalRGB/permissions` (partner permissions,
  all granted) and `@SignalRGB/systeminfo` (`MAG B550 TOMAHAWK (MS-7C91)` from SMBIOS).

## Dry-run limits

Dry-run suppresses writes, so request/response protocols get no answers: the MSI plugin's
EZ-LED probe (`0x01 0xBA` then a read) reports "EZ LED disabled", and the Razer plugin's
transaction-id probe never finds the mouse. Live runs are the reference for those paths.
