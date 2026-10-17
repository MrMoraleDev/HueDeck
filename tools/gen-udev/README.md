<!-- SPDX-License-Identifier: Apache-2.0 OR MIT -->
# tools/gen-udev

Generates `70-huedeck.rules` for Linux from the device plugins HueDeck knows about.
Planned for M7 (Linux backend); this directory holds the contract so the CLI
(`huedeck udev generate|install`) and the standalone tool agree.

## Input

- Every plugin loaded by the plugin index (first-party `plugins/`, imported SignalRGB
  plugins, community sources): `VendorId` + `ProductId` (int, int[] or decimal-string
  keys), `Type` (`Hid`, `rawusb`, `hybrid`, `serial`, `SMBUS`, `network`), `Name`.
- The template `packaging/udev/70-huedeck.rules.template` (per-device block between the
  `---- per-device block` markers, plus the fixed I2C tail).

## Output

One rules file, deterministic order (sorted by VID, then PID), one comment line per
plugin followed by the two `TAG+="uaccess"` lines (`usb` subsystem for libusb/rawusb and
`hidraw` for HID), with VID/PID rendered as 4-digit lowercase hex. Plugins of type
`SMBUS` contribute nothing per device; the single `KERNEL=="i2c-[0-9]*", GROUP="i2c"`
rule is always emitted. `serial` plugins add no rule (dialout group membership is the
documented requirement). `network` plugins are skipped.

Example output fragment:

```
# Razer DeathAdder V2 Pro (razer-deathadder-v2-pro)
SUBSYSTEMS=="usb", ATTRS{idVendor}=="1532", ATTRS{idProduct}=="007c", TAG+="uaccess"
KERNEL=="hidraw*", SUBSYSTEM=="hidraw", ATTRS{idVendor}=="1532", ATTRS{idProduct}=="007c", TAG+="uaccess"
```

## Planned interface

```
gen-udev [--plugins <dir>...] [--template <file>] [--out <file>] [--check]
```

- `--check` exits non-zero when the existing `--out` file differs (CI drift test).
- Shared implementation with `huedeck udev generate` (crate `huedeck-cli`); this tool is
  the headless entry point used by packaging scripts (AppImage, Flatpak post-install).
- Tests: golden file against the first-party plugin set, uniqueness of VID:PID lines,
  no uppercase hex, template markers present.
