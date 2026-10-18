<!-- SPDX-License-Identifier: GPL-3.0-or-later -->

# Sony DualSense (`054C:0CE6`): light bar over USB

The plugin is `plugins/sony-dualsense/`. It drives the light bar only, and only over USB.

## Sources

| # | Source | Used for |
|---|--------|----------|
| S1 | The Linux kernel's `drivers/hid/hid-playstation.c` (GPL-2.0-or-later), read 2026-10-18 | Field order of output report 0x02, the valid-flag bits, the light bar setup value and when the driver sends it |
| S2 | Lightloom dry-run of another plugin against this controller (`lightloom light --dry-run`, 2026-10-18) | The report the controller is used to on Windows: report id 0x02, 48 bytes, light bar colour at bytes 45..47, valid_flag2 at 39, lightbar_setup at 42 |
| S3 | Lightloom's device listing on the reference machine | Interface 3, usage page 0x0001, usage 0x0005 (game pad), output report length 48 |
| S4 | Live runs of the plugin on the reference machine (2026-10-18) | Every write accepted (see "Bench") |

S1 and S2 agree on every offset. The plugin contains no code from either.

## Transport

USB interface 3, the game pad collection (usage page `0x0001`, usage `0x0005`); output
report `0x02`, 48 bytes including the report id (Windows' output report length for this
collection; the Linux driver sends 63). Over Bluetooth the controller uses report `0x31`
with a checksum instead: not supported, and the plugin's `Validate()` does not match it.

## Output report 0x02 (USB)

Byte 0 is the report id. The fields the plugin uses are marked; it leaves every other byte 0.

| Byte | Field (S1 name) | Lightloom |
|------|-----------------|---------|
| 1 | `valid_flag0`: vibration emulation, haptics, triggers, headphone/speaker/mic volume, audio routing | **always 0** |
| 2 | `valid_flag1`: bit 0 mic mute LED, bit 1 power save, **bit 2 light bar colour**, bit 3 release LEDs, bit 4 player LEDs, bit 7 audio control 2 | `0x04` in colour reports |
| 3, 4 | rumble motors | 0 |
| 5..8 | headphone volume, speaker volume, mic volume, audio control | 0 (ignored: flag clear) |
| 9 | mute button LED | 0 (ignored) |
| 10 | power save control | 0 (ignored) |
| 11..37 | adaptive triggers and reserved | 0 |
| 38 | `audio_control2` | 0 |
| 39 | `valid_flag2`: **bit 1 light bar setup**, bit 2 vibration emulation 2 | `0x02` in the start report |
| 40, 41 | reserved | 0 |
| 42 | `lightbar_setup`: `0x02` = fade the controller's own light out | `0x02` in the start report |
| 43 | LED brightness | 0 (ignored) |
| 44 | player LEDs | 0 (ignored: flag clear) |
| 45, 46, 47 | light bar red, green, blue | the sampled colour |

## What the plugin sends

| When | Report (hex, 48 bytes, zeros elided) |
|------|--------------------------------------|
| `Initialize()` | `02`, byte 39 = `02`, byte 42 = `02`: light bar setup, so the host colour takes over (S1 sends it once at start for the same reason) |
| every frame | `02 00 04`, bytes 45..47 = R G B |
| `Shutdown()` | `02 00 04`, bytes 45..47 = `00 00 00` (light bar off; over USB it is off by default, S1) |

Every report keeps `valid_flag0` at 0 and sets no `valid_flag1`/`valid_flag2` bit other
than the light bar ones, so volume, audio routing, vibration, triggers, the microphone LED
and the player LEDs are never changed. The plugin's tests check this for every report.
The other plugin seen in S2 sets `valid_flag0` to `0xFC` (volumes and audio routing) and
`valid_flag1` to `0xD7`; Lightloom deliberately does not.

## Bench (2026-10-18, reference machine, SignalRGB closed)

| Run | Frames | Writes | Errors | Write p50 / p99 |
|-----|--------|--------|--------|-----------------|
| `lightloom light red` / `#00ff00` / `blue`, 4 s each | 341 | 347 | 0 | 1.8-2.1 / 3.2-3.6 ms |
| `lightloom effect effects/rainbow-wave.html`, 15 s | 432 | 434 | 0 | 2.0 / 4.0 ms |

The controller accepted every report. Whether the light bar showed the colours has not
been checked by eye yet, so the product id stays out of `tested` in the manifest until it
has.
