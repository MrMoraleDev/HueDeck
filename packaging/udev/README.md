<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# packaging/udev

`70-huedeck.rules` lets the unprivileged daemon open the devices the first-party plugins
claim. It is generated, never edited by hand:

```
huedeck udev generate --plugins plugins --out packaging/udev/70-huedeck.rules
```

A `huedeck-cli` test (`udev::tests::first_party_rules_match_the_packaged_file`) fails
when a manifest changes and the file is not regenerated. Contract and rule shapes:
`tools/gen-udev/README.md`.

## Install

```
sudo install -m 0644 70-huedeck.rules /etc/udev/rules.d/70-huedeck.rules
sudo install -m 0644 ../sysusers.d/huedeck.conf /etc/sysusers.d/huedeck.conf
sudo systemd-sysusers && sudo usermod -aG i2c "$USER"   # SMBus plugins (RAM, GPU) only
sudo udevadm control --reload-rules && sudo udevadm trigger
```

Packages use `/usr/lib/udev/rules.d/` and `/usr/lib/sysusers.d/` instead. Log in again
after joining the `i2c` group. Plugged-in devices get their ACL on the next `add` or
`change` event; the daemon re-enumerates on those events (hidraw hotplug).

With imported plugins (SignalRGB or community), generate a file from those directories
too: `huedeck udev generate --plugins plugins --plugins <dir> --out 70-huedeck.rules`.

## Without the rules

`huedeck devices` still lists every HID device (it reads only sysfs, which is
world-readable); opening one fails with "permission denied" and a pointer here.
