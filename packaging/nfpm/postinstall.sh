#!/bin/sh
# SPDX-License-Identifier: GPL-3.0-or-later
#
# Lightloom .deb / .rpm post-install (packaging/nfpm). Debian passes "configure", RPM 1 or 2.
# Creates the i2c group, loads i2c-dev and applies the udev rules to devices already plugged
# in. Never enables or starts the user service: each user runs
#   systemctl --user enable --now lightloomd
# Every step tolerates a system without systemd or udev (containers, chroots).

set -e

case "$1" in
  configure | 1 | 2) ;;
  *) exit 0 ;;
esac

if command -v systemd-sysusers >/dev/null 2>&1; then
  systemd-sysusers /usr/lib/sysusers.d/lightloom.conf || true
fi

# /dev/i2c-* for the SMBus plugins now; /usr/lib/modules-load.d/lightloom.conf loads it at boot.
if command -v modprobe >/dev/null 2>&1; then
  modprobe -q i2c-dev 2>/dev/null || true
fi

if [ -d /run/udev ] && command -v udevadm >/dev/null 2>&1; then
  udevadm control --reload || true
  udevadm trigger --action=change --subsystem-match=hidraw --subsystem-match=usb \
    --subsystem-match=tty --subsystem-match=i2c-dev || true
fi

exit 0
