<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# packaging/udev

`70-lightloom.rules` lets the unprivileged daemon open the devices the first-party plugins
claim. It is generated, never edited by hand:

```
lightloom udev generate --plugins plugins --out packaging/udev/70-lightloom.rules
```

A `lightloom-cli` test (`udev::tests::first_party_rules_match_the_packaged_file`) fails
when a manifest changes and the file is not regenerated. Contract and rule shapes:
`tools/gen-udev/README.md`.

## Install

```
sudo install -m 0644 70-lightloom.rules /etc/udev/rules.d/70-lightloom.rules
sudo install -m 0644 ../sysusers.d/lightloom.conf /etc/sysusers.d/lightloom.conf
sudo systemd-sysusers && sudo usermod -aG i2c "$USER"   # SMBus plugins (RAM, GPU) only
sudo install -m 0644 ../modules-load.d/lightloom.conf /etc/modules-load.d/lightloom.conf
sudo modprobe i2c-dev                                    # /dev/i2c-* nodes
sudo udevadm control --reload-rules && sudo udevadm trigger
```

Packages use `/usr/lib/udev/rules.d/`, `/usr/lib/sysusers.d/` and
`/usr/lib/modules-load.d/` instead. The `i2c` group gets only the adapters Lightloom
uses, matched by kernel adapter name (`ATTR{name}`): AMD PIIX4 ports 0 and 1, Intel
i801, NVIDIA port 1, the AMDGPU OEM bus. DDC, touchpad, EC and the other PIIX4 ports
stay root-only. Log in again
after joining the `i2c` group. Plugged-in devices get their ACL on the next `add` or
`change` event; the daemon re-enumerates on those events (hidraw hotplug).

With imported plugins (SignalRGB or community), generate a file from those directories
too: `lightloom udev generate --plugins plugins --plugins <dir> --out 70-lightloom.rules`.

## Without the rules

`lightloom devices` still lists every HID device (it reads only sysfs, which is
world-readable); opening one fails with "permission denied" and a pointer here.
