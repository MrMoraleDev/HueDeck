<!-- SPDX-License-Identifier: GPL-3.0-or-later -->

# "Needs WinUSB driver"

Some lighting devices are not HID devices: Lian Li Uni Fan hubs (original and SL 1.2), Aqua
Computer controllers, Thermalright and Turzx LCDs, the LCD streams of some AIO pumps. Their plugins
(`Type()` `rawusb` or `hybrid`) talk to the device with raw USB bulk and control transfers. On
Windows a program can do that only through a user-mode USB driver, and HueDeck uses **WinUSB**, the
one that ships with Windows.

HueDeck lists a device as **needs WinUSB driver** when none of its USB interfaces is bound to
WinUSB. The message says which driver owns each interface (`HidUsb`, `usbaudio`, nothing, ...).

## What HueDeck does and does not do

- It **never installs, replaces or generates a driver**, and ships no INF file. Replacing a
  device's driver can break its other functions (keyboard keys, audio, the vendor's own app) and
  needs a trusted certificate on the machine; see `docs/research/drivers.md` §2.3 for why every
  option was rejected for now.
- It **never claims an interface bound to another driver.** On a hybrid device (HID plus a vendor
  interface) the HID part keeps working through HID; only the WinUSB interface is used for bulk
  and control transfers.
- A `rawusb` device without WinUSB is not driven at all. A `hybrid` device without WinUSB still
  gets its HID lighting; the transfers its plugin needs fail, and the session says so.

## When does a device get WinUSB?

1. **By itself.** Windows binds WinUSB automatically to devices whose firmware declares it through
   Microsoft OS descriptors (MS OS 1.0 or 2.0, compatible id `WINUSB`). Nothing to do.
2. **Through the vendor's software.** Some vendor apps install WinUSB for their device. Close the
   vendor app while HueDeck drives the device: only one program can claim the interface.
3. **Otherwise** the device is not usable by HueDeck yet. Prefer a plugin that uses the device's
   HID interface, if it has one.

Drivers installed by Zadig or libwdi as **libusbK** or **libusb-win32** are not WinUSB and are not
reachable by HueDeck.

## How to check (no install needed)

1. Device Manager > View > Devices by connection. Find the device; if its vendor interface sits
   under "Universal Serial Bus devices" with Driver `WinUsb`, HueDeck can drive it.
2. Or run `pnputil /enum-devices /connected /drivers` and look for `winusb.inf` on its instance.
3. Report what you find (VID:PID, which interface, which driver) in the plugin's
   `docs/hardware/<plugin>.md` or an issue: only devices that bind WinUSB by themselves go into a
   rawusb plugin's `tested` list.
