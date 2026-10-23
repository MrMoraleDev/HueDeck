<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# tools/gen-udev

Generates `70-huedeck.rules` for Linux from the plugin manifests. The implementation is
the `huedeck udev generate` subcommand (`crates/huedeck-cli/src/udev.rs`); there is no
separate binary, so packaging scripts (AppImage, Flatpak post-install, distro packages)
call the CLI.

```
huedeck udev generate [--plugins <dir>...] [--out <file>] [--check]
```

- `--plugins`: a plugin directory (holding `manifest.json`) or a directory of plugin
  directories; repeatable. Default: `HUEDECK_PLUGINS`, else `./plugins`.
- `--out`: write the file instead of printing it.
- `--check`: write nothing; exit 1 when `--out` differs (drift test).

The packaged file is `packaging/udev/70-huedeck.rules` (install steps in
`packaging/udev/README.md`).

## Input

Every `manifest.json` (manifest v2, `schemas/plugin-manifest-v2.schema.json`):
`devices[]` with `vid` + `pids` (integers or hex strings, `"0x1462"` or `"1462"`) and
`transport` (`hid` by default, `rawusb`, `hybrid`, `serial`), `smbus` claims and
`hidUsage` claims.

## Output

Deterministic: plugins sorted by id, each with a `# <id>: <name>` comment, then its
VID:PID rules sorted by VID, PID; a VID:PID already emitted for an earlier plugin is not
repeated. Ids are 4-digit lower-case hex, as sysfs prints them.

| transport | rule |
|---|---|
| `hid` | `ACTION!="remove", SUBSYSTEM=="hidraw", ATTRS{idVendor}=="vvvv", ATTRS{idProduct}=="pppp", TAG+="uaccess"` |
| `rawusb` | `ACTION!="remove", SUBSYSTEM=="usb", ENV{DEVTYPE}=="usb_device", ATTR{idVendor}=="vvvv", ATTR{idProduct}=="pppp", TAG+="uaccess"` |
| `hybrid` | both of the above |
| `serial` | `ACTION!="remove", SUBSYSTEM=="tty", ATTRS{...}, TAG+="uaccess", ENV{ID_MM_DEVICE_IGNORE}="1"` |

Why this shape (docs/research/m7-linux-backend.md, section 0 finding 5 and section 1.6):

- Every rule names its subsystem. The old template's bare `SUBSYSTEMS=="usb"` match also
  tagged the device's `input` and `tty` nodes, which would hand the seat user raw
  keystrokes of a keyboard.
- `ACTION!="remove"`: since systemd 258 ACLs are re-applied on `change` events.
- The file name sorts below `73-seat-late.rules`, which applies the `uaccess` tag.
- Residual risk: the hidraw node of a keyboard's boot interface still carries keystrokes
  (OpenRGB has the same). A rule cannot combine `idVendor` (USB device) with
  `bInterfaceNumber` (USB interface): udev matches all `ATTRS` keys on one ancestor.

`hidUsage` claims (HID LampArray on any device) have no VID:PID and get a comment only;
`network` claims are skipped. SMBus claims add nothing per device; one rule always closes
the file:

```
ACTION!="remove", SUBSYSTEM=="i2c-dev", KERNEL=="i2c-[0-9]*", GROUP="i2c", MODE="0660"
```

The `i2c` group comes from `packaging/sysusers.d/huedeck.conf` (Debian's i2c-tools
creates the same group). It grants every I2C bus, including SPD and DDC; restricting it by
adapter (PCI class or vendor, as ddcutil does) is open work for WP-L4.

## Tests

`crates/huedeck-cli/src/udev.rs`: manifest id forms, rule scoping and ordering, `--check`,
and a golden test of the first-party plugin set against
`packaging/udev/70-huedeck.rules` (unique lines, lower-case hex, every rule scoped).
