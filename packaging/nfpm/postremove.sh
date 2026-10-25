#!/bin/sh
# SPDX-License-Identifier: GPL-3.0-or-later
#
# Lightloom .deb / .rpm post-remove (packaging/nfpm). Debian passes "remove" or "purge", RPM 0
# on removal (1 on upgrade: nothing to do). Drops the udev ACLs the removed rules gave. The i2c
# group stays: other tools (i2c-tools, OpenRGB) use it too. Users' own data under
# ~/.local/share/lightloom and ~/.config/lightloom is never touched.

set -e

case "$1" in
  remove | purge | 0) ;;
  *) exit 0 ;;
esac

if [ -d /run/udev ] && command -v udevadm >/dev/null 2>&1; then
  udevadm control --reload || true
  udevadm trigger --action=change --subsystem-match=hidraw --subsystem-match=usb \
    --subsystem-match=tty --subsystem-match=i2c-dev || true
fi

exit 0
