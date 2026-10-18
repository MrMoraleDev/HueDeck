<!-- SPDX-License-Identifier: Apache-2.0 OR MIT -->
<!-- Research brief for M7. Produced 2026-10-18 by a research pass and an independent fact-check pass; facts carry inline provenance tags. -->

# Lightloom M7: Linux backend research brief (topic: linux-backend), independently verified edition

**Scope:** hidraw/hidapi, nusb, SMBus over i2c-dev, SuperIO and hwmon fan control, CEF on Linux, PipeWire audio and screen capture, OS-lighting, coexistence and systemd.

**Sources and conventions**
- Verified 2026-10-18. An independent verifier re-fetched every primary source in §10 and did not reuse the original author's summaries. Verifier downloads: `<research-cache>` (subfolders `kernel\`, `systemd\`, `crates-api\`, `crates-src\`, `cef\`, `chromium\`, `nvidia\`, `openrgb\`, `misc\`). The original author's downloads are under `...\_downloads\linux-backend\`.
- **Kernel:** torvalds/linux `master` is 7.3-rc7 (Makefile `VERSION = 7`, `PATCHLEVEL = 3`, `EXTRAVERSION = -rc7`) [V:K-make]. A "since vX.Y" statement means the same file was fetched at tag vX.Y and at the previous tag [V:K-tags].
- **systemd:** `main` is `263~devel` [V:SD-ver].
- **Crates:** checked through the crates.io API plus the exact `.crate` tarballs, extracted and read.
- **Tags:**
  - [V:key]: verified against the source listed under that key in §10.
  - [V-repo]: read in the local repo on 2026-10-18. The file mtime is given where it matters.
  - [U]: unverified.
  - [design]: a recommendation, not a fact.
- **Licences:** GPL/LGPL material was read only for facts (names, numbers, behaviour). This covers the kernel, systemd, OpenRGB, i2c-tools, nct6687d and Debian packaging. Nothing from it may be copied into the permissive tree.

---
## 0. Top findings that change the plan or the repo

1. **One hidraw node per USB HID interface is confirmed, with two edge cases.**
   - `usbhid_probe(struct usb_interface *intf, ...)` calls `hid_allocate_device()` once per interface. hidraw attaches per `hid_device` (`hidraw_connect(struct hid_device *)`). All top-level collections (TLCs) of an interface therefore share one `/dev/hidrawN` and one report descriptor, and `lightloom-hid-desc` must split them. [V:K-usbhid][V:K-hidraw-c]
   - Edge case 1: `usbhid_probe` returns `-ENODEV` ("couldn't find an input interrupt endpoint") when the interface has no interrupt-IN endpoint. Such an interface gets no hidraw node. [V:K-usbhid]
   - Edge case 2: hidraw minors are a fixed system-wide pool, `HIDRAW_MAX_DEVICES = 64`. When the pool is exhausted, `hidraw_connect()` fails with `-EINVAL`. [V:K-hidraw-h][V:K-hidraw-c]
2. **Do not derive endpoint tuples from hidapi-rs 2.6.7 `linux-native` enumeration.** [V:CR-hidapi]
   - `next_hid_usage()` carries the comment "This comes from hidapi which apparently comes from Apple's implementation". It emits a `(usage_page, usage)` pair at every Collection item (`0xa0`) and does not track depth or collection type.
   - `device_to_hid_device_info()` returns `None` for any bus except USB 0x03, Bluetooth 0x05, I2C 0x18 and SPI 0x1C, so `BUS_VIRTUAL` (0x06) devices are invisible.
   - Use your own sysfs enumeration plus `lightloom-hid-desc`, and use hidapi only for `open_path` and I/O.
3. **The Windows `ColNN` field is hex, and the repo is already fixed.**
   - Microsoft's legend defines `b(2)` as "two hex digits", "0x00-0xFF", "Collection number (only used with multiple-TLC devices.)". [V:MS-hwid]
   - `crates/lightloom-hal-windows/src/path.rs` (mtime 2026-10-18 11:06) now parses `&col` with `u16::from_str_radix(h, 16)` and documents "Two hex digits (`Col0A` is the 10th TLC), never decimal". [V-repo]
   - The original finding ("the repo parses decimal") is obsolete.
   - Still open: SignalRGB's own `endpoint.collection` value for devices with 10 or more TLCs [U], and path.rs's statement that `hidclass.sys` formats the field as `Col%02X` [U].
4. **Linux needs TLC-handle parity rules that Windows HIDClass provides for free (§1.4).** [design, built on verified behaviour]
   - Demultiplex input reports by report ID.
   - Pad or truncate output and feature buffers the way hidapi windows-native does: `fill_buffer` computes `data_size = data.len().min(self.buffer.len())` and zero-fills the rest. [V:CR-hidapi]
   - Optionally reject report IDs that do not belong to the TLC. [U]
5. **The udev template is over-broad.**
   - `packaging/udev/70-lightloom.rules.template` emits `SUBSYSTEMS=="usb", ATTRS{idVendor}=="@VID@", ATTRS{idProduct}=="@PID@", TAG+="uaccess"` with no `SUBSYSTEM` or `KERNEL` restriction. [V-repo] It therefore matches every descendant node, including `/dev/input/event*` and `tty*`.
   - `73-seat-late.rules.in` applies `RUN{builtin}+="uaccess"` to every uaccess-tagged node with `ENV{MAJOR}!=""` [V:SD-seatlate]. The seat user would gain raw keyboard event access.
   - Fix: restrict the rules to `SUBSYSTEM=="hidraw"`, to `SUBSYSTEM=="usb", ENV{DEVTYPE}=="usb_device"`, and to `SUBSYSTEM=="tty"`.
   - The template has no `ACTION` filter, so it already fires on `change` events, which systemd 258 requires. Still adopt the recommended `ACTION!="remove"` form. [V:SD-NEWS]
   - Rule file names must sort below 73; `70-` is fine. [V:SD-NEWS]
6. **The i2c rule relies on a group that may not exist, and it is too broad.**
   - The template line is `KERNEL=="i2c-[0-9]*", GROUP="i2c", MODE="0660"` [V-repo]. systemd's `sysusers.d/basic.conf.in` defines no `i2c` and no `plugdev` group. [V:SD-basic]
   - Debian's `i2c-tools` package does create the group: `addgroup --quiet --system i2c` in the 4.3-2 postinst, and `g i2c` in the 4.4-3 sysusers file. It also ships `SUBSYSTEM=="i2c-dev",KERNEL=="i2c-[0-9]*", GROUP="i2c", MODE="0660"`. [V:DEB-i2ct] Whether Ubuntu carries the same packaging: [U].
   - Lightloom should ship its own `sysusers.d` group [V:SD-sysusers]. It should also restrict access by parent PCI class and vendor, as ddcutil does with `SUBSYSTEM=="i2c-dev", KERNEL=="i2c-[0-9]*", ATTRS{class}=="0x03*", TAG+="uaccess"` [V:DDC-rule]. The blanket rule also grants the SPD, DDC, touchpad and EC buses.
7. **SMBus capabilities differ per adapter, and i2cdev 0.6.2 has no `I2C_FUNCS` call.**
   - PIIX4 `piix4_func()` returns QUICK, BYTE, BYTE_DATA, WORD_DATA and BLOCK_DATA only, and the driver has no `master_xfer`. [V:K-piix4]
   - i2cdev 0.6.2 defines `const I2C_FUNCS: u16 = 0x0705` but generates no ioctl wrapper for it. Its `smbus_write_block_data` silently truncates to 32 bytes via `copy_to_i2c_block_data(values, 32)`, so validate `len <= 32` first. [V:CR-i2cdev]
   - `WriteBlock` is an SMBus Block Write with a count prefix. Sources: repo `docs/host-api/smbus.md` L43 [V-repo] and OpenRGB ENE's `i2c_smbus_write_block_data(dev, 0x03, sz, data)` [V:ORGB-ene].
   - `crates/lightloom-hal/src/smbus.rs` L134 still says "(I2C combined write: reg + data)" [V-repo]. Reconcile the two.
8. **The kernel claims DDR4 SPD addresses, but only from Linux 6.11.**
   - `i2c-piix4` registers SPD probing since v6.11; v6.10 has none. The call is `i2c_register_spd()`, renamed `i2c_register_spd_write_enable()` since v6.16. [V:K-tags]
   - ee1004 then binds 0x50+n and creates dummy clients at 0x36/0x37. [V:K-ee1004]
   - `I2C_SLAVE` returns `EBUSY` only when a client at that address has a driver bound (`i2cdev_check()` tests `dev->driver`). [V:K-i2cdev-c]
   - Consequence: the Patriot Viper plugin writes 0x36 and reads 0x50-0x57 [V-repo smbus.md L95]. It fails on kernels 6.11 and later. On Ubuntu 24.04's GA 6.8 it works unless ee1004 is instantiated by hand.
   - The Crucial plugin is unaffected: it remaps from source 0x27 into {0x39-0x3C, 0x20-0x23}, which is 0x39/0x3A on this machine. [V-repo smbus.md L90]
   - Never use `I2C_SLAVE_FORCE`.
9. **NCT6687D fan control is not possible with mainline drivers (confirmed), and there is a safety trap.** [V:K-nct6683-c][V:K-nct6775-rst][V:NCT-readme]
   - Mainline `nct6683` grants write access to `pwm` only for Mitac boards, and enforces this purely through the file mode: `nct6683_pwm_is_visible()` adds `S_IWUSR` only for customer ID 0xa0e. `store_pwm()` itself has no customer check.
   - A udev rule that runs `chmod g+w` on `pwm*` of any hwmon named `nct6687` would therefore unlock unguarded EC writes on the *mainline* driver.
   - Gate such a rule on the out-of-tree driver with `DRIVERS=="nct6687"`. Mainline's platform driver and device are named `nct6683` (§4.3).
   - `docs/architecture.md` §6 says "hwmon (`nct6775`, `it87`)" [V-repo L147]. That is wrong for this board.
10. **CEF on Linux has sandbox, WebGL and branch-support constraints.**
    - Ozone headless "draws graphical output to a PNG image (no GPU support; software rendering only)". [V:CH-ozone]
    - WebGL then needs `--enable-unsafe-swiftshader`, which the SwiftShader docs say is "not intended for running untrusted content". [V:CH-swiftshader]
    - Ubuntu 24.04's AppArmor user-namespace restriction needs a profile for the sandbox. [V:UB-2404]
    - AppImage docs offer only two options: enable unprivileged namespaces, or `--no-sandbox`. [V:AI-docs]
    - Flatpak needs zypak with `ZYPAK_CEF_LIBRARY_PATH`. [V:ZYPAK]
    - **New:** CEF's branch table lists 8037 (M154), the branch the repo pins (`cef = { version = "=154.5.0+154.0.34", default-features = false }` [V-repo]), under "Legacy Release Branches (Unsupported)". The supported branches are 8078/M156 (Beta), 7977/M152 (Extended, last refresh Oct 20, 2026) and 7871/M150 (LTC, until Apr 13, 2027). [V:CEF-branches]
    - crates.io also has `cef` 152.4.0+152.0.8 and 150.2.1+150.0.14. [V:CR-cef] Choose the CEF line before M7.
11. **Audio and screen capture can use crates already in the plan, with three new caveats.**
    - cpal 0.18.2 with the `pipewire` feature captures a sink by setting `STREAM_CAPTURE_SINK="true"`. [V:CR-cpal]
    - Screen capture: ashpd 0.13.13 ScreenCast plus pipewire 0.10, connecting with `Context::connect_fd(fd: OwnedFd, properties)`. [V:CR-ashpd][V:CR-pw]
    - **Never enable ashpd's `pipewire` feature.** It pulls pipewire 0.9, and both pipewire-sys 0.9.2 and 0.10.1 declare `links = "pipewire-0.3"`, so Cargo refuses the graph. [V:CR-pwsys]
    - **New:** on Linux, cpal always depends on `alsa` 0.11, which needs `alsa-sys` 0.4.0. Its build.rs requires pkg-config `alsa` (`libasound2-dev`), even when only the PipeWire host is used. [V:CR-cpal][V:CR-alsa]
    - **New:** ScreenCast v6 deprecates the stream node ID for targeting. Consumers should use the `pipewire-serial` property with `PW_KEY_TARGET_OBJECT`, which ashpd 0.13.13's `Stream` does not expose. [V:XDP-sc][V:CR-ashpd]
12. **The Linux dependency set needs Rust 1.87 or newer.** Bump the workspace `rust-version`, currently `1.85`. [V-repo Cargo.toml L9] [V:CR-ashpd][V:CR-zbus][V:CR-xcap]
    - ashpd 0.13.13 requires 1.87.
    - zbus 5.14.0 to 5.19.0 require 1.87 (5.13.x requires 1.85); zbus 6.0.0 requires 1.89.
    - xcap 0.9.8 requires zbus `^5.17` on Linux.
13. **Flatpak cannot provide everything.**
    - `/sys/{block,bus,class,dev,devices}` are `--ro-bind`, so no hwmon or LED writes are possible. [V:FP-run]
    - hidraw and i2c need `--device=all`. `--device=usb` binds only `/dev/bus/usb`, and `--device=input` binds only `/dev/input` ("raw and virtual input currently require --device=all"). [V:FP-run]
    - udev rules must be installed on the host by the user.
    - Flatpak has no PipeWire socket permission. Its sockets are x11, wayland, pulseaudio, session-bus, system-bus, fallback-x11, ssh-auth, pcsc, cups, gpg-agent and inherit-wayland-socket. [V:FP-ctx]
    - xdg-desktop-portal has no audio portal. [V:XDP-list] Use `--socket=pulseaudio` with cpal's `pulseaudio` host. [design]
    - udev hotplug needs the host network namespace. Netlink multicast is netns-scoped, and Flatpak adds `--unshare-net` unless network access is shared. [V:K-af-netlink][V:FP-run]
    - Repo attribution: the "Flatpak post-install" udev step is in `tools/gen-udev/README.md` L41. "Portals are used for screen capture (`engine.zone`) and audio (`engine.audio`)" is in `packaging/README.md`. [V-repo]
14. **Linux has no OS lighting API.** The closest equivalents are both kernel-provided:
    - The Tuxedo NB04 virtual HID LampArray (`BUS_VIRTUAL`, vendor 0x21ba, product 0x0400), since 6.16. [V:K-tuxedo][V:K-tags]
    - LED-class multicolor devices, for example `hid-steelseries` for MSI KLC 1038:1122 and ALC 1038:1161 (DMI match "Raider A18 HX A9WJG"). This is only in master 7.3-rc and absent in v7.2. [V:K-steelseries][V:K-tags]
15. **Repo and CI nits.**
    - CI installs `libudev-dev libusb-1.0-0-dev pkg-config`. [V-repo .github/workflows/ci.yml L50-52]
    - Nothing needs libusb: nusb is pure Rust (rustix plus linux-raw-sys), and hidapi `linux-native-basic-udev` is pure Rust. [V:CR-nusb][V:CR-hidapi]
    - Correction: `libudev-dev` is needed not only for the `udev` crate. The workspace pins `serialport = "4.10"` with its default `libudev` feature, which also needs it. [V-repo][V:CR-serialport]
    - Add `libasound2-dev` (for cpal), `libpipewire-0.3-dev` and `libclang-dev` (for pipewire-sys and bindgen), and `libxcb1-dev` (xcap uses xcb 1.7.1, which links only `xcb`). [V:CR-alsa][V:CR-pwsys][V:CR-xcb][V:UB-pkgs]
    - `lightloom-hal-linux` has `#![forbid(unsafe_code)]` [V-repo], but `I2C_FUNCS` and any self-made hidraw or usbfs ioctl need `unsafe`. Isolate them in an audited `lightloom-sys-linux` module or crate. [design]
16. **k10temp on the Ryzen 5 5600X exposes `Tctl` and `Tccd1`, not `Tdie`.** `Tdie` is shown only for CPUs in `tctl_offset_table`: Ryzen 1600X, 1700X, 1800X, 2700X and Threadripper 19xx/29xx. [V:K-k10temp]
17. **The i801 adapter name changed across kernels.** Parse all three forms. [V:K-tags]
    - Up to 6.11, including Ubuntu 24.04's 6.8: `"SMBus I801 adapter at %04lx"`.
    - 6.12 to 6.14: `"SMBus %s adapter at %04lx"`, where `%s` is `I801` or `I801 IDF`.
    - 6.15 and later: `"SMBus %s adapter at %s"`, using `pci_name()`.

### 0.1 Kernel/distro version matrix (each row checked at the stated tags) [V:K-tags][V:UB-2404][V:UB-2604]
| feature | first mainline | Ubuntu 24.04 GA (6.8) | Ubuntu 26.04 GA (7.0) |
|---|---|---|---|
| `HIDIOC{S,G}{INPUT,OUTPUT}`, `HID_MAX_BUFFER_SIZE` 16384 | 5.11 | yes | yes |
| `HIDIOCREVOKE` | 6.12 | no | yes |
| i2c-piix4 automatic SPD instantiation (ee1004/spd5118) | 6.11 | no | yes |
| `i2c-amd-asf-plat`, plus aux suppression on `\_SB.ASFC` | 6.13 | no | yes |
| i801 name `"SMBus %s adapter at <pci_name>"` | 6.15 | old form | new form |
| amdgpu `"AMDGPU DM i2c OEM bus"` | 6.15 | no | yes |
| Tuxedo NB04 virtual LampArray | 6.16 | no | yes |
| hid-steelseries MSI KLC/ALC | master (7.3-rc) | no | no |
| nct6683 MSI customer IDs | 6.8 has 0x200 and 0x201; 0x207 and 0x20d are present in 7.0 | partial | yes |
| ee1004 source path | `drivers/misc/eeprom/` up to 7.2; `drivers/nvmem/` in master | – | – |

- systemd: Ubuntu 24.04 ships 255.4, Ubuntu 26.04 ships 259.
- PipeWire: Ubuntu 24.04 shipped 1.0.4 at release; noble-updates `libpipewire-0.3-dev` is 1.0.5-1ubuntu3.3.
- Rust: Ubuntu 26.04 ships rustc 1.93. [V:UB-2604][V:UB-pkgs]

---
## 1. HID over hidraw

### 1.1 Kernel uapi (`include/uapi/linux/hidraw.h`) [V:K-hidraw-h]
- **Encoding (x86-64, asm-generic):** `dir<<30 | size<<16 | 'H'(0x48)<<8 | nr`, with READ=2 and WRITE=1.
- **Values** were computed from the header with LP64 sizes: `hidraw_report_descriptor` is 4100 B and `hidraw_devinfo` is 8 B. [V: computed]

| ioctl | definition | value (len=0; OR in `len<<16`) |
|---|---|---|
| HIDIOCGRDESCSIZE | `_IOR('H', 0x01, int)` | 0x80044801 |
| HIDIOCGRDESC | `_IOR('H', 0x02, struct hidraw_report_descriptor)` | 0x90044802 |
| HIDIOCGRAWINFO | `_IOR('H', 0x03, struct hidraw_devinfo)` | 0x80084803 |
| HIDIOCGRAWNAME / GRAWPHYS / GRAWUNIQ (len) | `_IOC(_IOC_READ, 'H', 0x04/0x05/0x08, len)` | 0x80004804 / 0x80004805 / 0x80004808 |
| HIDIOCSFEATURE / GFEATURE (len) | `_IOC(_IOC_WRITE\|_IOC_READ, 'H', 0x06/0x07, len)` | 0xC0004806 / 0xC0004807 |
| HIDIOCSINPUT / GINPUT / SOUTPUT / GOUTPUT (len) | same macro, nr 0x09 / 0x0A / 0x0B / 0x0C | 0xC0004809 / 0xC000480A / 0xC000480B / 0xC000480C |
| HIDIOCREVOKE | `_IOW('H', 0x0D, int)`, "Revoke device access"; argument must be 0, otherwise `EINVAL` | 0x4004480D |

- **Struct layouts:**
  - `struct hidraw_report_descriptor { __u32 size; __u8 value[HID_MAX_DESCRIPTOR_SIZE]; }`, with `HID_MAX_DESCRIPTOR_SIZE` = 4096. [V:K-hid-h]
  - `struct hidraw_devinfo { __u32 bustype; __s16 vendor; __s16 product; }`. Cast vendor and product to u16.
  - Constants: `HIDRAW_FIRST_MINOR 0`, `HIDRAW_MAX_DEVICES 64`, `HIDRAW_BUFFER_SIZE 64`.
- **Dispatch checks:** `HIDIOCTL_LAST = _IOC_NR(HIDIOCREVOKE)`. A type other than `'H'` gives `EINVAL`; nr 0 or above `HIDIOCTL_LAST` gives `ENOTTY`. [V:K-hidraw-h][V:K-hidraw-c]
- **Kernel versions:**
  - `HIDIOC{S,G}{INPUT,OUTPUT}` are absent in v5.10 and present in v5.11. They came from Dean Camera's patch "HID: hidraw: Add additional hidraw input/output report ioctls." [V:K-tags][V:K-511]
  - The same v5.11 HID pull raised `HID_MAX_BUFFER_SIZE` from 8192 to 16384. [V:K-511]
  - `HIDIOCREVOKE` is absent in v6.11 and present from v6.12. [V:K-tags]
- **`HIDIOCGRDESC`:** a `len` greater than 4095 gives `EINVAL`. Otherwise it copies `min(hid->rsize, len)` bytes of `hid->rdesc`. [V:K-hidraw-c]
- **Documentation caveat:**
  - The 5.11 patch rewrote hidraw.rst to say that feature reads include the report number, replacing the earlier "the report number is not returned".
  - The same text still says "For devices which do not use numbered reports, the report data will begin at the first byte of the returned buffer".
  - The usbhid code (§1.2) keeps `buf[0] = 0` and puts the data at `buf[1]`. Trust the code. [V:K-511][V:K-hidraw-rst][V:K-usbhid]

### 1.2 Runtime semantics [V:K-hidraw-c][V:K-usbhid][V:K-usb-h]
- **No exclusivity.**
  - Any number of opens is allowed. Each fd has a ring of 64 slots (63 usable: when `new_head == tail` the report is skipped).
  - `hidraw_report_event()` copies every input report to every non-revoked fd. When a reader's ring is full, the new report is dropped for that reader.
- **`read()`:**
  - For numbered reports, byte 0 is the report ID. Unnumbered reports have no 0x00 prefix.
  - Data beyond the caller's buffer is discarded.
  - Returns `EIO` once the device is gone, and `ENODEV` after `HIDIOCREVOKE`.
  - `poll()` sets `EPOLLERR|EPOLLHUP` when the device is gone or revoked, and always sets `EPOLLOUT`.
- **`write()`:**
  - A count below 2 or above 16384 gives `EINVAL`. `buf[0]` is the report ID, 0 if unnumbered.
  - It is sent through `__hid_hw_output_report()` to the interrupt-OUT endpoint. Only on `-ENOSYS` (usbhid without `urbout`) does it fall back to `SET_REPORT(Output)`.
  - The quirk `HID_QUIRK_NO_OUTPUT_REPORTS_ON_INTR_EP` forces SET_REPORT.
- **Report ioctls:**
  - `HIDIOCSOUTPUT` takes the same path as `write()` (interrupt-OUT first), although hidraw.rst calls it a control-endpoint transfer.
  - `HIDIOCSFEATURE` and `HIDIOCSINPUT` send `SET_REPORT`.
  - `HIDIOCG*` sends `GET_REPORT`. It needs `ll_driver->raw_request`, otherwise `ENODEV`, and returns `min(ret, len)`.
- **usbhid on the wire:**
  - A 0 report-ID byte is stripped for interrupt-OUT and SET_REPORT, and the returned count includes it.
  - `HID_QUIRK_SKIP_OUTPUT_REPORT_ID` forces ID 0 for output SET_REPORT.
  - `GET_REPORT` sets `buf[0] = report_number`. For ID 0, the data lands at `buf[1]` and the count includes the ID byte. This is the same convention as hidapi.
- **Timeouts:** usbhid uses `USB_CTRL_SET_TIMEOUT` (5000 ms) for GET_REPORT, SET_REPORT and interrupt-OUT; `USB_CTRL_GET_TIMEOUT` is also 5000 ms. A wedged device can block about 5 s per call, so keep device I/O off the 250 ms `Render()` watchdog path. [design; watchdog from plan]
- **Default node permissions:**
  - The hidraw class has no devnode callback, so nodes get the devtmpfs default, root:root 0600. [V:K-devtmpfs]
  - systemd's 50-udev-default only has `SUBSYSTEM=="hidraw", IMPORT{builtin}="hwdb"`, with no MODE or GROUP. [V:SD-default]

### 1.3 Enumeration without opening devices
For each `/sys/class/hidraw/hidrawN`:
- **`device/uevent`** (the parent `hid` device): [V:K-hidcore]
  - It holds `HID_ID=%04X:%08X:%08X` (bus:vendor:product, upper-case hex), `HID_NAME`, `HID_PHYS`, `HID_UNIQ` and `MODALIAS=hid:b%04Xg%04Xv%08Xp%08X`.
  - For USB, `HID_PHYS` is `usb-<bus_name>-<devpath>/input<bInterfaceNumber in decimal>`. `usb_make_path()` is `snprintf(buf, size, "usb-%s-%s", dev->bus->bus_name, dev->devpath)`. [V:K-usb-h][V:K-usbhid]
  - For USB, `HID_UNIQ` is the iSerialNumber string, empty when absent. [V:K-usbhid]
- **`device/report_descriptor`:**
  - A `BIN_ATTR_RO` (0444) attribute that returns `hdev->rdesc`.
  - `hid_open_report()` builds `rdesc` from `bpf_rdesc` (after HID-BPF fixups) and then from the bound driver's `report_fixup()`. `HIDIOCGRDESC` returns the same buffer, and the pristine `dev_rdesc` is not exported. [V:K-hidcore][V:K-hidraw-c]
  - So both views are post-fixup, and HID-BPF programs can change the bytes Lightloom parses.
  - Because the attribute is world-readable, Lightloom can parse descriptors and report "device present, no permission; install the udev rules" before any open. [design]
- **USB parents:**
  - `usb_interface/bInterfaceNumber` is printed as `"%02x\n"` (hex). [V:K-usb-sysfs] hidapi parses it with radix 16. [V:CR-hidapi]
  - `usb_device/idVendor`, `idProduct` and `bcdDevice` are printed as `"%04x\n"` (lower case). [V:K-usb-sysfs]
  - Also available: `manufacturer`, `product`, `serial`, `busnum`, `devnum` and `devpath`. [V:CR-nusb] That the string attributes are absent when the descriptor is missing: [U].
- **Bus types:** `BUS_USB` 0x03, `BUS_BLUETOOTH` 0x05, `BUS_VIRTUAL` 0x06, `BUS_I2C` 0x18, `BUS_HOST` 0x19, `BUS_SPI` 0x1C. [V:K-input-h] The interface is -1 for non-USB buses (hidapi convention). [V:CR-hidapi]
- **Grouping into physical devices:**
  - Group by the shared `usb_device` ancestor and key on `vid:pid:serial`.
  - SignalRGB keys serial-less devices by instance id, for example `054c:0ce6:6-00000000-0-10`. [V-repo docs/host-api/macros-system.md L185]
  - On Linux, fall back to the port chain (`devpath` or `ID_PATH`).
  - systemd 258 and later also creates `/dev/input/by-id/<ID_BUS>-<ID_SERIAL>[-if<NN>]-hidraw` and `/dev/input/by-path/<ID_PATH>-hidraw`. [V:SD-persist-hidraw][V:SD-NEWS] Ubuntu 26.04 ships systemd 259, so it has these links; 24.04 (255.4) does not.
  - Imported Windows layouts for serial-less devices need manual re-binding. [design]

### 1.4 TLC split and parity rules (design on verified behaviour)
- **Windows:** HIDClass creates one device node per TLC, with hardware IDs `HID\Vid_v(4)&Pid_d(4)&MI_z(2)&Colb(2)`, where `b` is hex. [V:MS-hwid] That collections are numbered in descriptor order is [U]; the repo's M2 evidence in the path.rs doc supports it.
- **Linux rule** (matches `crates/lightloom-hal/src/hid.rs` [V-repo]):
  - List the Application collections at depth 0, in order.
  - With one TLC, `collection = 0`; otherwise 1..N.
  - `interface` = bInterfaceNumber (a non-composite USB device gives 0), or -1 for non-USB.
  - Report lengths come from `lightloom-hid-desc` and include the ID byte.
- **TLC handle:** open one fd per TLC handle; multiple opens are allowed. [V:K-hidraw-c]
  - **Input demultiplexing:** keep only reports whose byte 0 is in the TLC's input-ID set. "Multi-TLC interfaces always use report IDs" is [U] (HID-spec reasoning).
  - **Output and feature length:** hidapi windows-native pads and truncates to the caps length. Its comment reads: "Windows expects the number of bytes which are in the _longest_ report (plus one for the report number)… caps.OutputReportByteLength". [V:CR-hidapi]
    - On Linux, pad or truncate to the TLC's maximum report length of that kind (+1). [design]
    - Whether Windows sends the caps length or the per-ID length on the wire: [U].
  - **Foreign report IDs:** reject output or feature report IDs not declared in the TLC and return -1. [design] That this mimics Windows validation: [U].
  - **Read prefix:** hidapi windows-native strips the 0x00 byte on unnumbered reads [V:CR-hidapi], and hidraw never adds one [V:K-hidraw-c]. Both platforms return the same bytes.
- **Fixtures:**
  - Record the Linux sysfs `report_descriptor` plus the Windows `(MI, Col, HidP caps)` per device, and golden-test `lightloom-hid-desc(linux bytes) == windows tuples`.
  - Do not use hidapi-windows `get_report_descriptor` as the reference: `descriptor::get_descriptor(&PreparsedData)` reconstructs the descriptor from preparsed data. [V:CR-hidapi]
- **Kernel drivers on the target devices:**
  - 1462:7C91, 1532:007C, 1038:12AA and 320F:5064 are absent from every driver table checked, so they bind to `hid-generic` with no `report_fixup`. [V:K-hid-ids][V:K-hid-quirks][V:K-hid-razer][V:K-steelseries][V:K-hid-evision]
    - `hid-ids.h` has none of these product IDs.
    - `hid-razer` binds only BlackWidow 0x010e, 0x011b and 0x010D.
    - `hid-steelseries` binds SRWS1 and MSI KLC/ALC.
    - `hid-evision` binds only 320F:5041 and 320F:226f.
  - NZXT **1E71:2007** (Kraken X53/X63/X73) is bound by mainline `nzxt-kraken3` with `hid_hw_start(hdev, HID_CONNECT_HIDRAW)`. hidraw stays available, but the driver also talks to the device. [V:K-kraken3]

### 1.5 hidapi 2.6.7 on Linux [V:CR-hidapi]
- **Licence and features:** MIT.
  - `linux-native = ["dep:udev", "dep:nix"]` uses udev 0.9, libudev-sys and the system libudev.
  - `linux-native-basic-udev = ["dep:basic-udev", "dep:nix"]` is pure Rust. Its build.rs prints `cargo:rustc-cfg=feature="linux-native"` so the same code runs.
  - The defaults are `["linux-static-hidraw", "illumos-static-libusb"]`, which compile the C hidapi and need libudev through pkg-config. Keep `default-features = false`.
  - build.rs panics with "Exactly one linux hidapi backend must be selected."
  - Linux dependencies: nix 0.30 (`fs`, `ioctl`, `poll`), udev 0.9 and basic-udev 0.1.
- **Device calls:**
  - `open_path`: opens read+write (O_RDWR) with `O_CLOEXEC|O_NONBLOCK`, then checks with `HIDIOCGRDESCSIZE` (error "not a HIDRAW device?").
  - `read_timeout`: `poll()` then `read()`. `EAGAIN` returns Ok(0); POLLERR, POLLHUP or POLLNVAL return "device disconnected".
  - `send_feature_report`: `HIDIOCSFEATURE`, returning `IncompleteSendError` unless all bytes were sent.
  - `get_feature_report`: `HIDIOCGFEATURE`.
  - `send_output_report`: `HIDIOCSOUTPUT`, which in practice tries interrupt-OUT first (§1.2).
  - `get_input_report`: `HIDIOCGINPUT`.
  - `get_report_descriptor`: reads sysfs `/sys/dev/char/M:m/device/report_descriptor`, not `HIDIOCGRDESC`.
  - `HidDevice` has `unsafe impl Send` and contains `Cell`/`RefCell` fields, so it is Send but not Sync.
- **Windows gap:** `HidDevice::get_input_report` is compiled only with `#[cfg(any(hidapi, target_os = "linux"))]`, so `windows-native` lacks it. This is why `crates/lightloom-hal-windows/src/hid.rs` L188-193 still has a TODO for `HidD_GetInputReport`. [V-repo]
- **Recommendation [design]:**
  - Declare `hidapi = { version = "=2.6.7", default-features = false, features = ["linux-native-basic-udev"] }` under `[target.'cfg(target_os = "linux")'.dependencies]`, in `lightloom-hal-linux` only.
  - The workspace already uses `resolver = "2"` [V-repo]. That this prevents the Windows crate's `cfg(windows)` `windows-native` feature from unifying into Linux builds is [U] (Cargo resolver semantics, not re-read).
  - Use `HidApi::new_without_enumerate()` plus `open_path()`. [V:CR-hidapi] Alternatively, own the ioctls in an unsafe-isolated module.

### 1.6 Permissions
**Facts**
- **uaccess application:** `73-seat-late.rules.in` starts with `ACTION=="remove", GOTO="seat_late_end"` and contains `TAG=="uaccess|xaccess-*", ENV{MAJOR}!="", RUN{builtin}+="uaccess"` inside `{% if HAVE_ACL %}`. [V:SD-seatlate]
- **systemd 258 changes** [V:SD-NEWS]:
  - ACLs "are now always applied/updated by systemd-udevd through "uaccess" udev builtin, and systemd-logind no longer applies/updates ACLs but triggers "change" uevents".
  - "the "uaccess" udev tag should be set not only on "add" action but also on "change" action". The recommended form is `ACTION!="remove", SUBSYSTEM=="hidraw", TAG+="uaccess"`.
  - "Udev rules files setting the "uaccess" tag need to have a file name starting with a number smaller than 73".
- **systemd 258 hwdb property `ID_AV_LIGHTS=`:** "may be set on USB controlled A/V lights… unprivileged users will get access to them". [V:SD-NEWS]
  - `70-uaccess.rules.in` contains `SUBSYSTEM=="hidraw", ENV{ID_AV_LIGHTS}=="1", TAG+="uaccess"` and the same rule for `usb`. [V:SD-uaccess]
  - `70-lights.hwdb` keys on `$MODALIAS` (for example `usb:v046DpC901*`), documents a local `/etc/udev/hwdb.d/71-lights-local.hwdb`, and is applied with `systemd-hwdb update` plus `udevadm trigger`. [V:SD-lights]
  - 50-udev-default imports the hwdb for hidraw and for usb_device. [V:SD-default]
  - The property is meant for A/V lights, so using it for RGB controllers stretches its intent. [design]
- **Distro versions:** Ubuntu 24.04 ships systemd 255.4, so it has neither change. [V:UB-2404] Ubuntu 26.04 ships systemd 259. [V:UB-2604]
- **systemd 257:** "logind now supports handing off session-managed access to hidraw devices via its D-Bus APIs". [V:SD-NEWS]
  - `TakeDevice(in u major, in u minor, out h fd, out b inactive)` "allows a session controller to get a file descriptor". The caller must be "the active session controller (see TakeControl())", and "any device can only be requested once". [V:SD-login1]
  - The daemon is not the session controller, so this path is unusable for it.
- **Default rules:** [V:SD-default]
  - `SUBSYSTEM=="usb", ENV{DEVTYPE}=="usb_device", MODE="0664"`.
  - `KERNEL=="tty[A-Z]*[0-9]|…", GROUP="dialout"`.
  - `SUBSYSTEM=="mem", KERNEL=="mem|kmem|port", GROUP="kmem", MODE="0640"`.
  - The default sysusers set has no `i2c` or `plugdev` group. [V:SD-basic]
- **OpenRGB precedent:**
  - "Starting with OpenRGB 1.0, the udev rules are now generated by OpenRGB itself" (`--generate-udev-rules <filename>` or `--print-udev-rules`). Packages install them at `/usr/lib/udev/rules.d/60-openrgb.rules`; manual installs use `/etc/udev/rules.d/60-openrgb.rules`. [V:ORGB-udevmd]
  - The generator writes `SUBSYSTEMS=="usb|hidraw", ATTRS{idVendor}=="%04x", ATTRS{idProduct}=="%04x", TAG+="uaccess", TAG+="<name>"`, plus `KERNEL=="i2c-[0-99]*", TAG+="uaccess"` and `KERNEL=="port", TAG+="uaccess"`. [V:ORGB-detect]
  - The `/dev/port` rule is ineffective: `open_port()` requires `CAP_SYS_RAWIO` and checks `security_locked_down(LOCKDOWN_DEV_MEM)`. [V:K-mem]

**Proposed generated rules [design]**
```
# /usr/lib/udev/rules.d/70-lightloom.rules (package) or /etc/udev/rules.d/70-lightloom.rules (manual)
ACTION!="remove", SUBSYSTEM=="hidraw", ATTRS{idVendor}=="1462", ATTRS{idProduct}=="7c91", TAG+="uaccess", GROUP="lightloom", MODE="0660"
ACTION!="remove", SUBSYSTEM=="usb", ENV{DEVTYPE}=="usb_device", ATTR{idVendor}=="1e71", ATTR{idProduct}=="2007", TAG+="uaccess", GROUP="lightloom", MODE="0660"
ACTION!="remove", SUBSYSTEM=="tty", ATTRS{idVendor}=="vvvv", ATTRS{idProduct}=="pppp", TAG+="uaccess", ENV{ID_MM_DEVICE_IGNORE}="1"
```
- Use lower-case hex in the rules to match sysfs `"%04x"`. [V:K-usb-sysfs]
- **Residual risk:** hidraw access to a keyboard's boot interface exposes keystrokes; OpenRGB has the same risk. Where the interface is known, add `ATTRS{bInterfaceNumber}=="02"` (two hex digits).
- **Headless fallback group:** ship `/usr/lib/sysusers.d/lightloom.conf` containing `g lightloom -`. [V:SD-sysusers] Whether a `plugdev` group exists differs per distro [U].
- **Optional, systemd 258 and later:** `/usr/lib/udev/hwdb.d/71-lightloom.hwdb` with the key `usb:v1462p7C91*` and the line ` ID_AV_LIGHTS=1`. [V:SD-lights mechanism]
- **ModemManager:** the NEWS entry in the 1.12.0 section says `ID_MM_DEVICE_IGNORE` "is instead targeted to users that want to explicitly ignore specific devices regardless of what filter type is in use (already in 1.10.6)". [V:MM-NEWS] The current docs list `MM_FILTER_RULE_EXPLICIT_BLOCKLIST` in the default STRICT policy. [V:MM-filter]

### 1.7 Hotplug
| option | facts |
|---|---|
| `udev` 0.9.3 (MIT) | `MonitorBuilder::new()` listens to the udev group, `new_kernel()` to raw kernel events. Also `.match_subsystem(..)`, `.match_subsystem_devtype(..)`, `.listen()` returning `Socket`, and `EventType::{Add, Change, Remove, Bind, Unbind, Unknown}`. Features: `hwdb`, `mio` (mio10), `send`, `sync`. Links libudev-sys 0.1.4 (MIT; `links = "libudev"`; pkg-config `libudev`, hence `libudev-dev`) to the system libudev, which is LGPL-2.1-or-later. Dynamic linking only. [V:CR-udev][V:CR-libudev-sys][V:SD-lic] |
| `tokio-udev` 0.10.0 (Apache-2.0 OR MIT) | Wraps udev 0.9 with tokio `net`. [V:CR-tokio-udev] |
| pure Rust, as nusb does | Netlink `KOBJECT_UEVENT`, bound with group mask `1 << 1` (= `MONITOR_GROUP_UDEV` 2). Packets start with `b"libudev\0"` and magic 0xfeedcafe in network order. In `monitor_netlink_header`, `properties_off` is at bytes 16..20 and `properties_len` at 20..24. nusb drops messages whose source group is not the udev group. [V:CR-nusb][V:SD-mon] |
| `basic-udev` 0.1.2 (MIT OR Apache-2.0) | Reads sysfs `uevent`. Provides `Enumerator` and `Device` only, no monitor. [V:CR-basic-udev] |
- **Use udev events, not raw kernel events.** Udev events arrive after the rules ran.
- **Subscribe to `change` events and retry opens that failed with `EACCES`.** Since systemd 258, ACLs are re-applied through `change` events. [V:SD-NEWS]
- **Flatpak:**
  - Current libudev `check_sender_uid()` accepts messages from uid 0, from the caller's own uid, or from uids outside the caller's user-namespace mapping. [V:SD-mon] Whether a given runtime's libudev has this logic: [U].
  - Netlink multicast is delivered only within the sender's network namespace (`net_eq()` in `do_one_broadcast()`). [V:K-af-netlink] Flatpak adds `--unshare-net` unless network access is shared [V:FP-run], so hotplug inside Flatpak needs `--share=network`.
  - Fallback: inotify on `/dev` [U].

## 2. Raw USB: nusb 0.2.8 [V:CR-nusb][V:K-usbfs-h][V:K-devio][V:K-usb-driver]
- **Crate:** Apache-2.0 OR MIT, `rust-version = "1.85"`, published 2026-10-14.
  - Linux dependencies are rustix 1.x (`fs`, `event`, `net`, `time`, `mm`) and linux-raw-sys; there is no libusb.
  - Enumeration reads sysfs, so it needs no permission.
- **Device nodes:** opens `/dev/bus/usb/{busnum:03}/{devnum:03}` with `OFlags::RDWR | OFlags::CLOEXEC`.
- **API:**
  - Listing and opening: `nusb::list_devices()` returns `impl MaybeFuture<Output = Result<impl Iterator<Item = DeviceInfo>, Error>>` (call `.wait()` to block). `DeviceInfo::open()` also returns a MaybeFuture.
  - Interfaces: `Device::claim_interface(n)` and `detach_and_claim_interface(n)` return MaybeFutures.
  - Kernel drivers: `detach_kernel_driver(n)` and `attach_kernel_driver(n)` are synchronous and take effect on Linux only.
  - Control transfers: `Device::control_in(ControlIn, Duration)` and `control_out(ControlOut, Duration)` work without claiming an interface on Linux, macOS, Android and wasm, but not on Windows.
  - Bulk: `Interface::endpoint::<Bulk, In|Out>(address)`.
  - Other: `Device::from_fd(OwnedFd)` (Linux and Android; for example fds from xdg-desktop-portal) and `nusb::watch_devices()`, which filters on `SUBSYSTEM=usb`, `DEVTYPE=usb_device`.
- **Internals:**
  - `detach_and_claim_interface` sends `USBDEVFS_DISCONNECT_CLAIM` with flag `USBDEVFS_DISCONNECT_CLAIM_EXCEPT_DRIVER` and driver `"usbfs"`.
  - `detach_kernel_driver` sends `USBDEVFS_IOCTL{ioctl_code = USBDEVFS_DISCONNECT}`.
  - `attach_kernel_driver` sends `USBDEVFS_IOCTL{USBDEVFS_CONNECT}`, which calls `device_attach()`.
- **usbfs ioctls** (x86-64 values computed from the header):
  - `USBDEVFS_CONTROL` = 0xC0185500: `_IOWR('U',0,struct usbdevfs_ctrltransfer{__u8 bRequestType; __u8 bRequest; __u16 wValue; __u16 wIndex; __u16 wLength; __u32 timeout /*ms*/; void *data})`.
  - `USBDEVFS_BULK` = 0xC0185502: `_IOWR('U',2,{unsigned ep, len, timeout; void *data})`.
  - `USBDEVFS_SUBMITURB` = 0x8038550A: `_IOR('U',10,struct usbdevfs_urb)`, a 56-byte struct.
  - `USBDEVFS_CLAIMINTERFACE` = 0x8004550F: `_IOR('U',15,unsigned int)`. `RELEASEINTERFACE` (nr 16) = 0x80045510.
  - `USBDEVFS_IOCTL` = 0xC0105512: `_IOWR('U',18,{int ifno; int ioctl_code; void *data})`.
  - `USBDEVFS_DISCONNECT` = 0x5516: `_IO('U',22)`. `USBDEVFS_CONNECT` = 0x5517: `_IO('U',23)`.
  - `USBDEVFS_GET_CAPABILITIES` = 0x8004551A: `_IOR('U',26,__u32)`.
  - `USBDEVFS_DISCONNECT_CLAIM` = 0x8108551B: `_IOR('U',27,{unsigned interface; unsigned flags; char driver[256]})`, with `IF_DRIVER` 0x01 and `EXCEPT_DRIVER` 0x02.
  - `USBDEVFS_DROP_PRIVILEGES` = 0x4004551E: `_IOW('U',30,__u32)`. After it, `USBDEVFS_IOCTL` and disconnect-claim of a driver-bound interface return `EACCES`.
- **Permissions:**
  - Every usbfs ioctl needs the fd opened for writing (`FMODE_WRITE`), otherwise `EPERM`.
  - `DISCONNECT` and `CONNECT` have no capability check. They fail only when privileges were dropped (`EACCES`) or the device is not configured (`EHOSTUNREACH`).
  - So the uaccess or group rule from §1.6 is sufficient. `usb_device` nodes default to 0664 root, which is not writable. [V:SD-default]
- **Exclusivity:**
  - `usb_driver_claim_interface()` returns `-EBUSY` when `iface->dev.driver` is set. Claiming an interface that usbhid still holds therefore fails with `EBUSY`. A second process's usbfs claim on the same interface also gets `EBUSY`.
  - `DISCONNECT_CLAIM` with `EXCEPT_DRIVER "usbfs"` returns `EBUSY` instead of stealing from another usbfs user.
  - `claimintf()` suppresses uevents while claiming.
- **`setProtocol('hybrid')` flow [design]:**
  1. Close the hidraw fds for the interface.
  2. Call `detach_and_claim_interface(n)`. usbhid unbinds and the hidraw node is removed; suppress the resulting `remove` event, because Lightloom caused it.
  3. Run the transfers.
  4. On Shutdown, release the interface and call `attach_kernel_driver(n)`. hidraw comes back, possibly with a new N.
- **Flatpak:**
  - `--device=usb` binds `/dev/bus/usb`. [V:FP-run]
  - The `org.freedesktop.portal.Usb` docs describe "version 1 of this interface" (EnumerateDevices, AcquireDevices, FinishAcquireDevices). [V:XDP-usb] Use it with the ashpd `usb` feature [V:CR-ashpd] and `Device::from_fd`.
  - Flatpak also has `--usb=VENDOR_ID:PRODUCT_ID` ("Add USB device to enumerables"). [V:FP-ctx]
  - Minimum portal and Flatpak versions: [U].

## 3. SMBus / I2C on Linux
### 3.1 Adapters and names
- **AMD FCH `i2c-piix4`** (PCI 1022:790B = `PCI_DEVICE_ID_AMD_KERNCZ_SMBUS`): [V:K-piix4][V:K-pci-ids]
  - Name format: `"SMBus PIIX4 adapter%s at %04x"`. Main port names are `" port 0", " port 2", " port 3", " port 4"`; the aux name is `" port 1"`.
  - `HUDSON2_MAIN_PORTS 2 /* HUDSON2, KERNCZ reserves ports 3, 4 */`. The resulting names are **`SMBus PIIX4 adapter port 0 at 0b00`**, **`SMBus PIIX4 adapter port 2 at 0b00`** and the aux **`SMBus PIIX4 adapter port 1 at 0b20`**.
  - Base address: for KERNCZ revision 0x49 and later (and Hygon), `smb_en = 0`, so main = `smba_en_hi << 8` and aux = main `| 0x20`.
  - SignalRGB on this machine logs PIIX4 buses at `b00` and `b20` [V-repo smbus.md L112], consistent with aux at 0x0B20.
  - The aux adapter is skipped when ACPI has `\_SB.ASFC` (`SB800_ASF_ACPI_PATH`; comment "Do not setup AUX port if ASF is enabled").
- **AMD ASF adapter** (`i2c-amd-asf-plat`): ACPI ID `AMDI001A`, name `"AMD ASF adapter"`. [V:K-asf][V:K-tags]
  - Functionality: `WRITE_BLOCK_DATA|BLOCK_DATA|BYTE|PEC|SLAVE`.
  - Copyright 2024; first in v6.13, absent in v6.12.
  - `amd_asf_xfer()` rejects every read with "ASF: Read not supported" (`EOPNOTSUPP`) and sends writes as SMBus block writes with PEC enabled. **Treat the ASF adapter as unusable for plugins.**
  - Whether the B550 Tomahawk BIOS has `\_SB.ASFC`: [U].
- **Other piix4 details:** KERNCZ revision 0x51 and later uses MMIO instead of ports 0xCD6/0xCD7 ("cd6h/cd7h port I/O accesses can be disabled"). Every base passes `acpi_check_region()`. [V:K-piix4]
- **SignalRGB parity:** SignalRGB exposes `AMD PIIX4 Bus - 0x0b00` and `- 0x0b20`, and "every plugin Scan runs on both". [V-repo smbus.md L112] Expose `port 0 @0b00` and aux `port 1 @0b20` under those names, and keep `port 2` opt-in. [design]
- **Intel i801:** see top finding 17 [V:K-tags]; functionality in §3.4. [V:K-i801]
- **NVIDIA** (open-gpu-kernel-modules 615.78.08, `kernel-open/nvidia/nv-i2c.c`; the README says "This code base is shared with NVIDIA's proprietary drivers"): [V:NV-i2c][V:NV-readme]
  - Name: `"NVIDIA i2c adapter %u at %x:%02x.%u\n"` (port, bus, slot, function), with a trailing `\n`. Without PCI it is `"NVIDIA SOC i2c adapter %u\n"`.
  - `dev.parent = nvl->dev`. The algorithm has both `.master_xfer` and `.smbus_xfer`.
  - Licence: the kernel module is dual MIT/GPLv2. [V:NV-copying]
  - OpenRGB's MSI GPU detector accepts NVIDIA buses only when `port_id == 1`. It lists 10DE:2504 + 1462:3903 as "MSI GeForce RTX 3060 Gaming X Trio LHR" at 0x68. [V:ORGB-msigpu][V:ORGB-pciids] Whether this card behaves accordingly: [U].
  - `nv.c` contains no hwmon code. [V:NV-nv.c] The rest of the module: [U].
- **amdgpu:** [V:K-amdgpu-conn][V:K-navi10][V:K-tags][V:PHX-oem][V:ORGB-amdgpu]
  - Names: `"AMDGPU DM i2c hw bus %d"` and `"AMDGPU DM i2c OEM bus"` (both now in amdgpu_dm_connector.c; `I2C_FUNC_I2C | I2C_FUNC_SMBUS_EMUL`), plus `"AMDGPU SMU %d"` (navi10_ppt.c).
  - The OEM bus first appears in v6.15 (absent in v6.14). Phoronix (2025-01-08) describes it as used "for implementing RGB lighting controls".
  - OpenRGB recognises `"AMDGPU DM i2c OEM bus"` and `"AMDGPU i2c bit bus OEM 0x97"`.
  - Map these buses to `IsAMDGpuBus()` only. [design]
- **Nuvoton SuperIO SMBus (`IsNuvotonBus`):** no mainline driver exists. [V:K-busses-mk][V:K-busses-kconfig][V:ORGB-smbus]
  - `drivers/i2c/busses/Makefile` has only `i2c-nct6694` ("NCT6694, a USB to I2C interface") and `i2c-npcm7xx` (NPCM BMC).
  - OpenRGB points to the out-of-tree `i2c-nct6793-dkms`.
  - These buses are absent on Linux.
- **`i2c-nvidia-gpu`** (`CONFIG_I2C_NVIDIA_GPU`) is "used to communicate with the GPU's Type-C controller" and selects `I2C_CCGX_UCSI`. It is not an RGB bus. [V:K-busses-kconfig][V:K-nvidia-gpu]
- **Adapter numbers are dynamic:** "They can even change from one boot to the next". [V:K-devif] Key caches by name plus parent PCI BDF plus port.

### 3.2 Identifying an adapter by PCI id from sysfs [V:ORGB-i2clinux facts][V:K-devif][V:K-pci-sysfs][V:K-i2cdev-c]
1. Iterate `/sys/bus/i2c/devices/i2c-*` (or `/sys/class/i2c-dev/i2c-*/device`).
2. Read `name` and trim the trailing `\n`.
3. Parse the port: `"SMBus PIIX4 adapter port %hu at"` or `"NVIDIA i2c adapter %hu at"`. OpenRGB also parses `"AMDGPU DM aux hw bus %hu"`.
4. `realpath` the entry and walk up to the first directory that has both `vendor` and `device`.
5. Read `vendor`, `device`, `subsystem_vendor` and `subsystem_device` (format `"0x%04x\n"`), and `class` (format `"0x%06x\n"`).
6. The node is `/dev/i2c-N`: character major 89, class `i2c-dev`, node name `i2c-%d`.

On this machine, expect PIIX4 adapters under `0x1022:0x790b` with class `0x0c05xx` (`PCI_CLASS_SERIAL_SMBUS 0x0c05`; the prog-if byte is [U]). Expect the NVIDIA adapter under `0x10de:0x2504` with subsystem `0x1462:0x3903`.

### 3.3 i2c-dev uapi [V:K-i2cdev-h][V:K-i2c-h][V:K-i2cdev-c][V:K-smbus-core][V:K-i2c-core-base]
- **ioctls:**
  - `I2C_RETRIES` 0x0701.
  - `I2C_TIMEOUT` 0x0702, in units of 10 ms (`adapter->timeout = msecs_to_jiffies(arg * 10)`).
  - `I2C_SLAVE` 0x0703 and `I2C_SLAVE_FORCE` 0x0706.
  - `I2C_TENBIT` 0x0704.
  - `I2C_FUNCS` 0x0705, which does `put_user(funcs, (unsigned long __user *)arg)`.
  - `I2C_RDWR` 0x0707, taking `struct i2c_rdwr_ioctl_data{struct i2c_msg *msgs; __u32 nmsgs}`. At most `I2C_RDWR_IOCTL_MAX_MSGS` (42) messages, each at most 8192 bytes.
  - `I2C_PEC` 0x0708.
  - `I2C_SMBUS` 0x0720, taking `struct i2c_smbus_ioctl_data{__u8 read_write; __u8 command; __u32 size; union i2c_smbus_data *data}`.
- **`I2C_SLAVE` behaviour:** returns `EBUSY` only when a client at that address, on this adapter or its mux parents or children, has `dev->driver` bound. `I2C_SLAVE_FORCE` skips the check. Neither needs a capability.
- **Data structures:**
  - `union i2c_smbus_data{__u8 byte; __u16 word; __u8 block[I2C_SMBUS_BLOCK_MAX + 2];}`, where `block[0]` is the length and `I2C_SMBUS_BLOCK_MAX` is 32.
  - `struct i2c_msg{__u16 addr; __u16 flags; __u16 len; __u8 *buf}`, with `I2C_M_RD` = 0x0001.
- **Sizes:** QUICK 0, BYTE 1, BYTE_DATA 2, WORD_DATA 3, PROC_CALL 4, BLOCK_DATA 5, I2C_BLOCK_BROKEN 6, BLOCK_PROC_CALL 7, I2C_BLOCK_DATA 8. `I2C_SMBUS_READ` = 1, `WRITE` = 0.
- **Functionality bits:**
  - `I2C_FUNC_I2C` 0x1, `PEC` 0x8, `SLAVE` 0x20.
  - `SMBUS_QUICK` 0x10000, `READ_BYTE` 0x20000, `WRITE_BYTE` 0x40000.
  - `READ_BYTE_DATA` 0x80000, `WRITE_BYTE_DATA` 0x100000, `READ_WORD_DATA` 0x200000, `WRITE_WORD_DATA` 0x400000, `PROC_CALL` 0x800000.
  - `READ_BLOCK_DATA` 0x1000000, `WRITE_BLOCK_DATA` 0x2000000, `READ_I2C_BLOCK` 0x4000000, `WRITE_I2C_BLOCK` 0x8000000, `HOST_NOTIFY` 0x10000000.
  - `I2C_FUNC_SMBUS_EMUL` = QUICK|BYTE|BYTE_DATA|WORD_DATA|PROC_CALL|WRITE_BLOCK_DATA|I2C_BLOCK|PEC.
- **Plain `read()` and `write()`** are capped at 8192 bytes and go through `i2c_transfer()`. `__i2c_transfer()` returns `-EOPNOTSUPP` ("I2C level transfers not supported") when the adapter has no `master_xfer`. This applies to PIIX4 and i801, and to `I2C_RDWR` on them too.
- **Locking:** `i2c_smbus_xfer()` takes the adapter lock (`__i2c_lock_bus_helper`, then `i2c_unlock_bus(…, I2C_LOCK_SEGMENT)`) for each transaction only. There is no cross-process lock comparable to Windows' `Global\Access_SMBUS.HTP.Method`.
  - Map `TransactionLock` and `TransactionUnlock` to an in-process per-bus mutex, optionally plus `flock()` on the node, which only Lightloom would honour. [design]
- **Emulation fallback:**
  - If `smbus_xfer` returns `-EOPNOTSUPP` and the adapter has `master_xfer`, the core emulates the transfer over I2C.
  - PIIX4 has no `master_xfer` (`.smbus_xfer = piix4_access[_sb800]`, `.functionality = piix4_func`). Unsupported sizes therefore fail with `EOPNOTSUPP` and log `dev_warn(&adap->dev, "Unsupported transaction %d\n", size)` on every call.
  - i2c-dev's `I2C_SMBUS` path does not check functionality first, so always check `I2C_FUNCS`.
- **PIIX4 errno values:** [V:K-piix4]
  - `ENXIO`: "no response" (status bit 0x04).
  - `EIO`: "Failed bus transaction" (0x10), or "Bus collision! SMBus may be locked until next hard reset. (sorry!)" (0x08).
  - `ETIMEDOUT`: "SMBus Timeout!".
  - `EBUSY`: host busy ("Failed! (%02x)"), hardware semaphore, or IMC region busy.
  - The IMC is notified only for `I2C_SMBUS_BLOCK_DATA` when `notify_imc` is set, which happens on KERNCZ with an active IMC (00:14.3 config byte 0x40, bit 7).

### 3.4 Capability matrix and SignalRGB mapping [V: func() bodies; ASF from xfer body]
| SignalRGB call | Linux op | PIIX4 | i801 | NVIDIA | AMDGPU DM | AMD ASF |
|---|---|---|---|---|---|---|
| WriteQuick(a) | SMBus QUICK write (`smbus_write_quick(false)`) | ✓ | ✓ | ✓ if SMBus-capable (keep NvAPI parity: deny) | ✓ per mask (emulated; zero-length message support [U]) | ✗ |
| ReadByte/WriteByte(a,reg[,v]) | BYTE_DATA | ✓ | ✓ | ✓ | ✓ | ✗ |
| ReadWord/WriteWord | WORD_DATA | ✓ | ✓ | ✓ | ✓ | ✗ |
| WriteBlock(a,reg,len,data) | BLOCK_DATA write, `block[0]=len`, len ≤ 32 | ✓ | ✓ | ✓ | ✓ (WRITE_BLOCK_DATA in EMUL) | ✗ (writes are sent with PEC appended; exclude the adapter) |
| ReadBlockBytes(a,reg,len) | I2C_BLOCK_DATA read, or I2C_RDWR write+read | ✗ | ✓ only with FEATURE_I2C_BLOCK_READ | ✓ | ✓ | ✗ |
| Write/ReadBlockWithoutRegister | I2C_RDWR single message (needs `I2C_FUNC_I2C`) | ✗ | ✗ | ✓ | ✓ | ✗ |
| ReadByteWithoutRegister (NvAPI only) | SMBus BYTE read | ✓ (parity: deny) | ✓ (deny) | ✓ | ✓ | ✗ ("ASF: Read not supported") |

- **Functionality masks:**
  - PIIX4: `QUICK|BYTE|BYTE_DATA|WORD_DATA|BLOCK_DATA`.
  - i801: those plus `PROC_CALL|WRITE_I2C_BLOCK`, and conditionally `PEC`, `BLOCK_PROC_CALL`, `READ_I2C_BLOCK` and `HOST_NOTIFY`. [V:K-i801]
  - NVIDIA: `I2C_FUNC_I2C`, plus, when `rm_i2c_is_smbus_capable`, `QUICK|BYTE|BYTE_DATA|WORD_DATA|BLOCK_DATA|I2C_BLOCK`. [V:NV-i2c]
  - AMDGPU DM: `I2C_FUNC_I2C|SMBUS_EMUL`. [V:K-amdgpu-conn]
  - ASF: `WRITE_BLOCK_DATA|BLOCK_DATA|BYTE|PEC|SLAVE`, although only writes work. [V:K-asf]
- **i2cdev 0.6.2 API** (licence `MIT/Apache-2.0`; nix 0.26.2): [V:CR-i2cdev]
  - `LinuxI2CDevice::new(path, slave_address: u16)` opens read+write, then issues `I2C_SLAVE` and `I2C_PEC`=0. `unsafe fn force_new(..)` uses `I2C_SLAVE_FORCE`. Also `set_slave_address(u16)` and `set_smbus_pec(bool)`.
  - Trait `I2CDevice`: `read(&mut [u8])`, `write(&[u8])`, `smbus_write_quick(bit: bool)` (false means `I2C_SMBUS_WRITE`), `smbus_read_byte()`, `smbus_write_byte(u8)`.
  - Data calls: `smbus_read_byte_data(reg)`, `smbus_write_byte_data(reg, val)`, `smbus_read_word_data(reg)`, `smbus_write_word_data(reg, u16)` and `smbus_process_word`.
  - Block calls: `smbus_read_block_data(reg) -> Vec<u8>`, `smbus_read_i2c_block_data(reg, len)`, `smbus_write_block_data(reg, &[u8])` (silently truncates to 32), `smbus_write_i2c_block_data(reg, &[u8])` and `smbus_process_block`.
  - `LinuxI2CBus::new(path)` plus `I2CTransfer::transfer(&mut [msgs]) -> Result<u32, _>` for I2C_RDWR.
  - There is no `I2C_FUNCS` wrapper, only the constant.
- **`WriteBlock` semantics:**
  - OpenRGB's ENE code does `i2c_smbus_write_word_data(dev, 0x00, swap(reg))`, then `i2c_smbus_write_block_data(dev, 0x03, sz, data)`, falling back to `i2c_smbus_write_byte_data(dev, 0x01, b)` per byte on -1. [V:ORGB-ene]
  - This matches SignalRGB's `ENE_RAM.js:331` `WriteBlock(address, 0x03, data.length, data)` [V-repo smbus.md], so **WriteBlock is an SMBus Block Write with a count prefix**.
  - SignalRGB's PIIX4 path has a "Starting Byte by Byte transaction" mode whose wire format is unknown. [V-repo smbus.md L235]
  - Update the `lightloom-hal` `SmbusBus::write_block` doc to match.
  - The capability predicates must be computed per adapter from `I2C_FUNCS` *and* the parity policy, not from `SmbusBusKind` alone (today the repo has `supports_write_quick = kind != Nvidia`). [V-repo]

### 3.5 Kernel-claimed addresses and safety
- **piix4 SPD registration** (since v6.11): `piix4_add_adapter()` runs `if (port == 0) i2c_register_spd_write_enable(adap);`. It is called with `port = 0` for main port 0 *and* for the aux adapter (`piix4_add_adapter(dev, retval, false, 0, false, 1, …)`), despite the comment "The AUX bus can not be probed…". [V:K-piix4][V:K-tags]
- **`i2c_register_spd()` rules:** [V:K-i2c-smbus][V:K-tags]
  - All filled DMI memory slots must share one type ("Different memory types mixed, not instantiating SPD").
  - Master clamps to 8 slots (`slot_count = min(slot_count, 8)`); v6.8 instead bailed out above 8.
  - Driver choice: DDR, DDR2, DDR3 and LPDDR1-3 use `spd`; DDR4 and LPDDR4 use `ee1004`; DDR5 and LPDDR5 use `spd5118` (skipped when write-disabled).
  - It probes 0x50+n with `i2c_new_scanned_device()`. The core's default probe uses Receive Byte, not Quick, for 0x30-0x37 and 0x50-0x5F. [V:K-i2c-core-base]
- **ee1004** (`drivers/nvmem/ee1004.c` in master; `drivers/misc/eeprom/ee1004.c` through v7.2): [V:K-ee1004][V:K-nvmem-core][V:K-i2c-core-base]
  - Selects pages "by writing a command to I2C address 0x36 or 0x37" through `i2c_new_dummy_device(adapter, 0x36 + cnr)`.
  - Caches `current_page` and skips the write when the page is unchanged. **Never write 0x36/0x37**: it desynchronises that cache.
  - The nvmem config is `read_only = true`, `root_only = false`, `compat = true`, `base_dev = &client->dev`. This gives the legacy file `/sys/bus/i2c/devices/<bus>-0050/eeprom` (client name `"%d-%04x"`) with mode 0444 (`0400 | 0044`), only when `CONFIG_NVMEM_SYSFS` is set.
- **i2cdetect** prints `UU` when `ioctl(file, I2C_SLAVE, …)` returns `EBUSY`. In auto mode it uses Receive Byte on 0x30-0x37 and 0x50-0x5F and Quick Write elsewhere ("This is known to corrupt the Atmel AT24RF08 EEPROM"). [V:I2CT-detect]
- Lightloom's denylist of writes to 0x30-0x37 and 0x50-0x57 covers these addresses; include WriteQuick, which is a write. [V-plan]
- **DDR5:** OpenRGB says spd5118 "can claim certain I2C addresses for Kingston Fury DDR5" and advises `rmmod spd5118`. [V:ORGB-smbus]

### 3.6 ACPI conflicts and module loading [V]
- **`acpi_enforce_resources=`** takes `strict`, `lax` or `no`. [V:K-params]
  - `strict` (default): "legacy drivers trying to access reserved resources will fail to bind".
  - `lax`: they "will bind successfully but a warning message is logged".
- **Affected drivers:** `i2c-piix4` calls `acpi_check_region` on each base, and `nct6683` calls `acpi_check_resource_conflict` on the EC base +4 (`IOREGION_OFFSET 4`, `IOREGION_LENGTH 4`). [V:K-piix4][V:K-nct6683-c]
  - OpenRGB: "Some Gigabyte/Aorus motherboards have an ACPI conflict". [V:ORGB-smbus] Whether the B550 Tomahawk has a conflict: [U].
- **Modules:**
  - Load `i2c-dev` through modules-load.d. Packages should ship `/usr/lib/modules-load.d/lightloom.conf` containing `i2c-dev`; that directory is in the search path. [V:SD-modload][V:ORGB-smbus]
  - `i2c-piix4` has `MODULE_DEVICE_TABLE(pci, piix4_ids)` including `PCI_DEVICE(PCI_VENDOR_ID_AMD, PCI_DEVICE_ID_AMD_KERNCZ_SMBUS)`, and systemd's `80-drivers.rules` runs `ENV{MODALIAS}=="?*", RUN{builtin}+="kmod load"`, so it autoloads. [V:K-piix4][V:SD-80drivers] That Ubuntu builds it as a module: [U].

### 3.7 Permissions for `/dev/i2c-*` (design, with verified precedents)
- **Precedents:**
  - ddcutil 2.2.3 ships `SUBSYSTEM=="i2c-dev", KERNEL=="i2c-[0-9]*", ATTRS{class}=="0x03*", TAG+="uaccess"`. [V:DDC-rule]
  - Debian i2c-tools ships `SUBSYSTEM=="i2c-dev",KERNEL=="i2c-[0-9]*", GROUP="i2c", MODE="0660"`. [V:DEB-i2ct]
- **Proposed Lightloom rules** (all `ATTRS` keys must match on the same parent, here the PCI device):
```
ACTION!="remove", SUBSYSTEM=="i2c-dev", KERNEL=="i2c-[0-9]*", ATTRS{class}=="0x0c05*", ATTRS{vendor}=="0x1022|0x8086", TAG+="uaccess", GROUP="lightloom", MODE="0660"
ACTION!="remove", SUBSYSTEM=="i2c-dev", KERNEL=="i2c-[0-9]*", ATTRS{class}=="0x03*", ATTRS{vendor}=="0x10de|0x1002", TAG+="uaccess", GROUP="lightloom", MODE="0660"
```
  Use `0x0c05*` rather than `0x0c0500`, because the prog-if byte is [U]. [V:K-pci-sysfs format]
- **Residual risk:** any process of that user can write to the chipset bus, including SPD. The daemon's allowlist is advisory against other processes, and OpenRGB's blanket rule has the same risk. Severity: [U].

## 4. SuperIO, fans and sensors
### 4.1 NCT6687D on the MSI B550 Tomahawk
- **Mainline `nct6683`** supports the NCT6683D, NCT6686D and NCT6687D (`SIO_NCT6683_ID 0xc730`, `SIO_NCT6686_ID 0xd440`, `SIO_NCT6687_ID 0xd590`, mask `0xFFF0`). [V:K-nct6683-c][V:K-nct6683-rst]
  - It instantiates only for known customer IDs, including MSI 0x201, 0x200, 0x207 and 0x20d (0x207 and 0x20d are not in v6.8); others need `force=1`.
  - The docs list "MSI B550 — NCT6687D EC firmware version 1.0 build 05/07/20".
  - The hwmon name is `nct6687`, but the platform driver and device are named `nct6683` (for example `nct6683.<addr>`).
  - Exposed attributes: `in*_{input,min,max,label}`, `fan*_{input,pulses,min}`, `temp*_{input,label,min,max,max_hyst,crit,type}` and `pwm*`.
  - `pwm*` is read-only unless the customer ID is Mitac (0xa0e), and there is no `pwm*_enable`.
  - `store_pwm()` itself has no Mitac check (see top finding 9).
- **`nct6775` does not support the NCT6687D.** Its documentation lists NCT6102D/6104D/6106D, NCT5572D/6771F/6772F/6775F/W83677HG-I, NCT5573D/5577D/6776D/6776F, NCT5532D/6779D, NCT6791D-6798D, NCT5585D and NCT6796D-S/6799D-R. [V:K-nct6775-rst]
- **Out-of-tree `Fred78290/nct6687d`** (GPL-2.0 per the GitHub licence field; repo last pushed 2026-10-13; README last changed 2026-09-08): [V:NCT-readme][V:NCT-repo]
  - Installed through DKMS (`.deb`) or akmod (`.rpm`) as module `nct6687`, platform device `nct6687.2592` (2592 = 0xa20).
  - Tested on the MAG-B550-TOMAHAWK (Ubuntu 20.04, 3900X/5900X).
  - **`pwm[1-8]`** takes 0-255, and "Writing to this file changes fan control to manual mode".
  - **`pwm[1-8]_enable`**: 1 = manual, 2 = firmware automatic. The legacy value 99 is accepted on write, but reads return 1 or 2.
  - Parameters: `force`, `manual`, `fan_config=default|msi_alt1`, `msi_fan_brute_force` [BETA], `fan_mask` (default 0xff) and `temp_mask` (default 0x7f).
  - `fan_control_watchdog` (a 1-300 s lease; 0 disarms it) exists only with `msi_fan_brute_force`. The supported-board table is limited to B840, B850, X870, X870E and Z890, so it does not apply to the B550.
  - Known issues: "ACPI: OSL: Resource conflict", fixed with `acpi_enforce_resources=lax`; "EC base I/O port unconfigured", fixed with `softdep nct6687 pre: i2c_i801`; `blacklist nct6683` is required for `msi_fan_brute_force`.
  - The hwmonN path is unstable. Locate the device under `/sys/devices/platform/nct6687.*/hwmon/hwmon*`.
  - Secure Boot and MOK signing for the DKMS build: [U] (the README says nothing about it).
- **Lightloom rules [design]:**
  - Detect the driver by parent driver name (`nct6687` vs `nct6683`) and by capability (`pwmN_enable` exists and `pwmN` is writable), not by the hwmon `name`.
  - Never use raw port I/O: `/dev/port` needs `CAP_SYS_RAWIO` and is blocked under lockdown. [V:K-mem]

### 4.2 hwmon ABI [V:K-hwmon-abi]
- `pwmY`: read-write, "Integer value in the range 0 to 255", "255 is max or 100%".
- `pwmY_enable`: 0 = "no fan speed control (i.e. fan at full speed)", 1 = manual through `pwmY`, 2 and above = automatic (chip-specific).
- `pwmY_mode`: 0 = DC, 1 = PWM.
- `fanY_input`: RPM, read-only.
- `tempY_input`: millidegrees Celsius, read-only.

**Sensors for curves:**
- `k10temp` labels are `Tctl`, `Tdie`, `Tccd1`..`Tccd12`. For family 19h model 0x21 ("Zen3 Ryzen Desktop", the 5600X) it shows `Tctl` and the valid CCDs (`Tccd1`). `Tdie` appears only for `tctl_offset_table` CPUs. [V:K-k10temp]
- `nzxt-kraken3` for 1E71:2007 has the hwmon name `x53` and driver `nzxt_kraken3`. [V:K-kraken3][V:K-kraken3-rst]
  - Attributes: `fan1_input` (pump RPM), `temp1_input` (coolant, m°C), `pwm1` (pump duty 0-255), `pwm1_enable` (0 = set to 100% and stop driver control, 1 = manual, 2 = curve) and `temp[1-2]_auto_point[1-40]_pwm`.
  - `fan2_input`, `pwm2` and `pwm2_enable` exist only on Z-series and Kraken 2023.
- NVIDIA GPU temperature needs NVML through `nvml-wrapper` 0.13.0 (MIT OR Apache-2.0). It loads `libnvidia-ml.so.1` at runtime through libloading 0.8. [V:CR-nvml]

### 4.3 Write permission and daemon algorithm (design)
- **Permissions:** hwmon attributes with a store function are root-owned 0644 (`S_IRUGO|S_IWUSR`; verified for nct6683 [V:K-nct6683-c]; the out-of-tree driver's modes are [U]).
- **Precedent:** brightnessctl uses `ACTION=="add", SUBSYSTEM=="backlight", RUN+="/bin/chgrp video /sys/class/backlight/%k/brightness"` plus `/bin/chmod g+w`, and the same for `leds` with group `input`. [V:BCTL-rule]
- **Proposed rule, restricted to the out-of-tree driver:**
  `ACTION=="add", SUBSYSTEM=="hwmon", ATTR{name}=="nct6687", DRIVERS=="nct6687", RUN+="/bin/sh -c 'chgrp lightloom /sys%p/pwm[1-8] /sys%p/pwm[1-8]_enable; chmod g+w /sys%p/pwm[1-8] /sys%p/pwm[1-8]_enable'"`
  - Never apply it to the mainline `nct6683` driver. Its `store_pwm()` writes EC registers unguarded once the mode allows. [V:K-nct6683-c]
  - Device-attribute files use read-write kernfs ops whenever the sysfs ops have both show and store [V:K-sysfs-file]. The file mode is the only gate.
  - That `DRIVERS==` matches at hwmon `add` time: [U].
- **Flatpak cannot do this**, because sysfs is read-only there. [V:FP-run]
- **Control loop:**
  1. Record each channel's `pwmN_enable` and `pwmN`.
  2. Set `pwmN_enable=1`, then write `pwmN` at no more than 1 Hz.
  3. Apply hysteresis and a minimum-duty floor, and force 255 at the critical temperature.
  4. Read back and re-assert manual mode if the firmware resets it.
  5. On exit or suspend, restore the recorded state (2 = firmware automatic).
- **Crash safety:**
  - `ExecStopPost=` runs "after the service is stopped", including when it "exited unexpectedly". [V:SD-service]
  - Combine it with `Type=notify` and `WatchdogSec=`.
  - sd-notify 0.5.0 (MIT OR Apache-2.0, MSRV 1.82) provides `sd_notify::notify(&[NotifyState::Ready])`, `NotifyState::Watchdog` ("WATCHDOG=1") and `watchdog_enabled() -> Option<Duration>`. [V:CR-sdnotify]

## 5. CEF on Linux
- **Version and support:**
  - The `cef` crate 154.5.0+154.0.34 (Apache-2.0 OR MIT, published 2026-10-07) wraps `154.0.34+g14c5a08+chromium-154.0.8037.98`. The Spotify index labels it `stable`, with linux64 files dated 2026-10-05. [V:CR-cef][V:CEF-index]
  - CEF tracks "even-numbered Chromium milestone branches". Branch **8037 (M154) is listed under "Legacy Release Branches (Unsupported)"**, with Linux build requirement "Ubuntu 20.04+, Debian 10+, Ninja". [V:CEF-branches]
  - Supported branches: Beta 8078/M156, Extended 7977/M152 (last refresh Oct 20, 2026) and LTC 7871/M150 (until Apr 13, 2027).
  - Newest index entries: 152.0.14 (stable, 2026-10-17), 156.0.7 (beta, 2026-10-11) and 150.0.22 (stable, 2026-10-09). [V:CEF-index]
- **Archive sizes (bz2):**
  - linux64 standard 689.4 MB, **minimal 326.5 MB**, client 326.5 MB, tools 427.7 MB.
  - There is no linux release-symbols archive.
  - For comparison, windows64 minimal is 172.8 MB. [V:CEF-index]
  - That `libcef.so` is unstripped, and the stripped size: [U].
- **Runtime files:**
  - README.redistrib (Linux): required `libcef.so`, `icudtl.dat`, `v8_context_snapshot.bin`; optional `locales/`, `chrome_100_percent.pak`, `chrome_200_percent.pak`, `resources.pak`; SwANGLE `libvk_swiftshader.so`, `libvulkan.so.1`, `vk_swiftshader_icd.json`. [V:CEF-redistrib]
  - CMake `CEF_BINARY_FILES`: `chrome-sandbox`, `libcef.so`, `libvk_swiftshader.so`, `libvulkan.so.1`, `v8_context_snapshot.bin`, `vk_swiftshader_icd.json`. [V:CEF-cmake]
  - The documented layout puts `chrome-sandbox` next to the executable. [V:CEF-usage]
- **System libraries:**
  - `dist_package_versions.json` covers only Debian 11/12 and Ubuntu 18.04/20.04/22.04. Its Debian 11 list: libasound2, libatk-bridge2.0-0, libatk1.0-0, libatspi2.0-0, libc6, libcairo2, libcups2, libdbus-1-3, libdrm2, libexpat1, libgbm1, libglib2.0-0, libnspr4, libnss3, libpango-1.0-0, libpangocairo-1.0-0, libstdc++6, libudev1, libuuid1, libx11-6, libx11-xcb1, libxcb-dri3-0, libxcb1, libxcomposite1, libxcursor1, libxdamage1, libxext6, libxfixes3, libxi6, libxkbcommon0, libxrandr2, libxrender1, libxshmfence1, libxss1, libxtst6. [V:CH-debdeps]
  - `additional_deps`: ca-certificates, fonts-liberation, "GTK is dlopen()'ed. Versions 3 or 4 are supported." (`libgtk-3-0 (>= 3.9.10) | libgtk-4-1`), `libnss3 (>= 2:3.35)`, wget, libcurl variants, libvulkan1, `xdg-utils (>= 1.0.2)`. [V:CH-adddeps]
  - Ubuntu 24.04 has `libasound2t64` (1.2.11). [V:UB-pkgs]
  - That `libcef.so`'s `DT_NEEDED` set matches Chrome's: [U]. Generate the real list with `dpkg-shlibdeps`. [design]
- **Ozone:**
  - Linux enables the X11, Wayland and Headless backends by default. Headless "draws graphical output to a PNG image (no GPU support; software rendering only)". [V:CH-ozone]
  - CEF OSR has been run with `--ozone-platform=headless --off-screen-rendering-enabled` (issue #3902, M133, Ubuntu 22.04). The crash in that issue was traced by the reporter to an unset `cef_key_event_t.size`; the issue was closed as completed on 2025-06-25. Set `.size` on every struct you pass in. [V:CEF-3902]
  - CEF maintainer, 2020-01-21: "A supported WM backend (x11, wayland, etc) is required". On 2020-01-22, about ozone headless: "We don't currently support that in CEF but it might be possible to get it working with code changes". On 2022-03-31: CEF works with Ozone X11 and Wayland, and Wayland needs Aura/Views. [V:CEF-forum1]
  - The 2019 thread (CEF-forum2) is by "Czarek", not the maintainer: "CEF depends on X11 library even in windowless mode", and he suggests Xvfb. [V:CEF-forum2]
  - Shared-texture OSR now defaults to `--use-angle=gl-egl` and `--ozone-platform=x11` (fixed 2025-07-11) and still fails on NVIDIA's GBM path. [V:CEF-3953]
- **Recommended renderer setup [design]:**
  - Use `--ozone-platform=headless` with CPU paint at 320x200.
  - Run WebGL lightscripts in a separate instance with `--use-gl=angle --use-angle=swiftshader-webgl --enable-unsafe-swiftshader`. [V:CH-swiftshader]
  - The automatic SwiftShader WebGL fallback has been deprecated since Chrome 130. The "Intent to Remove: SwiftShader Fallback" (2025-02-13) says "Shipping on Desktop 137". [V:CH-blinkdev]
  - CI: try headless first and fall back to `xvfb-run`.
- **Sandbox:**
  - Chrome uses "setuid(2) everywhere", "namespaces where supported" and "seccomp(2) BPF where supported". `--disable-setuid-sandbox` disables the setuid layer. [V:CH-sandbox]
  - The SUID helper must be set up with `sudo chown root:root … && sudo chmod 4755 …`. [V:CH-suid]
  - Ubuntu 24.04 restricts unprivileged user namespaces through AppArmor. The release notes give the example file `/etc/apparmor.d/chrome`: `abi <abi/4.0>,` / `include <tunables/global>` / `/opt/google/chrome/chrome flags=(unconfined) {` / `userns,` / `include if exists <local/chrome>` / `}`. [V:UB-2404]
  - The 23.10 blog uses `/etc/apparmor.d/opt.google.chrome.chrome` with `flags=(default_allow)` and names the sysctls `kernel.apparmor_restrict_unprivileged_userns` and `kernel.apparmor_restrict_unprivileged_unconfined`. [V:UB-blog]
  - Ship an equivalent profile for `/opt/lightloom/lightloom-renderer` in the .deb [design]. That it works unchanged: [U].
  - AppImage: the docs offer only "enable unprivileged namespaces" (`kernel.unprivileged_userns_clone=1`) or `--no-sandbox`. [V:AI-docs] That the random `/tmp/.mount_*` path defeats AppArmor path matching: [U].
  - Flatpak: zypak (BSD-3-Clause) runs on Platform 21.08 or later and is included in `org.electronjs.Electron2.BaseApp`. "If the application uses CEF, set `ZYPAK_CEF_LIBRARY_PATH` to the absolute path to the `libcef.so` library." [V:ZYPAK]
- **cef-rs on Linux:**
  - README: `cargo run -p export-cef-dir -- --force $HOME/.local/share/cef`, `export CEF_PATH="$HOME/.local/share/cef"`, `export LD_LIBRARY_PATH="$LD_LIBRARY_PATH:$CEF_PATH"`, and `cargo run --bin bundle-cef-app -- cefsimple -o target/bundle [--release]`. [V:CEFRS-readme]
  - `build_util::linux::bundle` copies the CEF directory (files only) and `locales` next to the binary.
  - The `Sandbox` type exists only on macOS: it loads `libcef_sandbox.dylib` and is `#[cfg(target_os = "macos")] pub mod sandbox`.
  - The `linux-x11` feature does nothing in the library. The bundler only forwards `-F linux-x11` to the app's `cargo build`. [V:CR-cef]
- **Packaging matrix [design]:**
  - **.deb / .rpm** (primary for the Ubuntu LTS acceptance test): `/opt/lightloom/*` with `chrome-sandbox` root 4755, the AppArmor profile, udev rules, hwdb, sysusers.d, modules-load.d and `/usr/lib/systemd/user/lightloomd.service`.
  - **AppImage:** the user installs the rules (`lightloom udev print | sudo tee /etc/udev/rules.d/70-lightloom.rules`), mirroring OpenRGB. [V:ORGB-udevmd] The sandbox is degraded on Ubuntu 24.04 and later.
  - **Flatpak:** `--device=all`, `--share=network` (for udev hotplug), `--socket=pulseaudio`, zypak, no fan or LED writes, and host-installed rules. Autostart through the Background portal (ashpd `background` feature [V:CR-ashpd]); its behaviour is [U].

## 6. Audio and screen capture
- **Audio with cpal 0.18.2** (Apache-2.0, MSRV 1.85): [V:CR-cpal]
  - The CHANGELOG for 0.18.0 says: "New host for Linux and BSD via the native PipeWire API" and "via the PulseAudio API", and "Default host priority: PipeWire > PulseAudio > ALSA". `default_host()` tries PipeWire, then PulseAudio, then ALSA.
  - Features: `pipewire` (pipewire 0.10 with `v0_3_53`) and `pulseaudio` (pure-Rust `pulseaudio` 0.3.2, MIT, "A native rust implementation of the PulseAudio protocol"). [V:CR-pulseaudio]
  - Discovered `Audio/Sink` nodes are exposed as `Duplex`. An input stream on a sink sets `STREAM_CAPTURE_SINK="true"` and `TARGET_OBJECT=<object_serial>`.
  - The PulseAudio host lists all sources through `list_sources()`. Picking `<default sink>.monitor`: [U].
  - **Mandatory on Linux:** `alsa` 0.11, which needs `alsa-sys` 0.4.0 (MIT, `links = "alsa"`). Its build.rs runs pkg-config `alsa` and says "apt-get install libasound2-dev". [V:CR-alsa]
- **Build and runtime dependencies:**
  - pipewire-sys 0.10.1: `bindgen 0.72.0` (feature `runtime`, needs libclang) and `system-deps 7.0.5` (pkg-config `libpipewire-0.3`), with `links = "pipewire-0.3"`. [V:CR-pwsys]
  - libpipewire licence: "All PipeWire source files are licensed under the MIT License", except `libspa-alsa.so` (LGPL) and `libjackserver.so` (GPL2). [V:PW-lic]
  - Ubuntu 24.04 ships PipeWire 1.0.4 [V:UB-2404]; noble-updates `libpipewire-0.3-dev` is 1.0.5. [V:UB-pkgs]
- **Screen capture on Wayland: ashpd 0.13.13** (MIT, MSRV 1.87; `zbus` `5.13` with `default-features = false`; optional `pipewire` 0.9, never enable it). [V:CR-ashpd]
  - Features: `default-features = false, features = ["tokio", "screencast"]`. The default feature set is `["tokio"]`.
  - Call sequence:
    1. `Screencast::new().await?`
    2. `create_session(Default::default())`
    3. `select_sources(&session, SelectSourcesOptions::default().set_sources(BitFlags::from(SourceType::Monitor)).set_cursor_mode(CursorMode::Hidden).set_persist_mode(PersistMode::ExplicitlyRevoked).set_restore_token(tok))`
    4. `start(&session, None, Default::default()).await?.response()?`, which gives `streams()[i].pipe_wire_node_id()` and `restore_token()`
    5. `open_pipe_wire_remote(&session, Default::default())`, which returns `OwnedFd`
  - `set_sources` takes `impl Into<Option<BitFlags<SourceType>>>`. Whether passing a bare `SourceType::Monitor` compiles: [U].
  - Connect with pipewire 0.10.1: `Context::connect_fd(&self, fd: OwnedFd, properties: Option<PropertiesBox>) -> Result<CoreBox<'_>, Error>`, or `ContextRc::connect_fd_rc`. [V:CR-pw]
  - **Portal semantics (interface version 6):** [V:XDP-sc]
    - `persist_mode` and `restore_token` have existed since v4.
    - "Restore tokens are single-use: each new session must use the token from the previous one". Store the newest token in SQLite.
    - `types` "Default is MONITOR".
    - Since v6, "the node ID in the stream tuple is deprecated for stream targeting… Consumers should prefer the `pipewire-serial` property… and `PW_KEY_TARGET_OBJECT`".
    - ashpd 0.13.13's `Stream` exposes `pipe_wire_node_id`, `position`, `size`, `source_type`, `id` and `mapping_id`, but no serial. Read the serial through raw zbus if needed. [V:CR-ashpd]
  - Negotiate `VideoFormat::{RGB, RGBA, RGBx, BGRx}`, as xcap does. [V:CR-xcap] Asking for memfd/SHM buffers to avoid DMA-BUF: [U].
- **xcap 0.9.8 on Linux** (Apache-2.0): [V:CR-xcap]
  - Dependencies: `xcb` 1.7 with `randr`, `zbus` 5.17, `pipewire` 0.10, `libwayshot-xcap` 0.3.3 (BSD-2-Clause [V:CR-wayshot]), `rand`, `url`, `serde`, `percent-encoding`.
  - `xcb` 1.7.1 (MIT) links only `libxcb` (`#[link(name = "xcb")]`, plus `X11-xcb` for the xlib bridge) unless the `dl` feature is enabled. [V:CR-xcb]
  - Wayland screenshots try `org.gnome.Shell.Screenshot`, then the portal Screenshot (`interactive=false`), then wlroots through libwayshot.
  - `VideoRecorder`'s `SelectSources` sends only `types=1` and `multiple=false`, with no `persist_mode` or `restore_token`. The picker therefore appears on every start, which rules it out for an autostarted daemon.
  - Recommendation [design]: use xcap's xcb grabs on X11 and your own ashpd plus pipewire path on Wayland; downscale to 160x100 and 28x20 as on Windows.
- **Flatpak:** use `--socket=pulseaudio`. There is no `pipewire` socket permission [V:FP-ctx] and no audio portal [V:XDP-list]. Native PipeWire through `--filesystem=xdg-run/pipewire-0`: [U].

## 7. No Dynamic Lighting on Linux; coexistence and systemd
- **OS-level equivalents:** there is no OS lighting API.
  - HID LampArray devices (usage page 0x59) are plain HID. The kernel also synthesises them: `tuxedo/nb04/wmi_ab.c` (`MODULE_DESCRIPTION("Virtual HID LampArray interface for TUXEDO NB04 devices")`, `hdev->bus = BUS_VIRTUAL; vendor = 0x21ba; product = 0x0400`; since 6.16). These are invisible to hidapi enumeration (§0.2), so include `BUS_VIRTUAL` in your own enumeration. [V:K-tuxedo][V:K-tags]
  - LED-class multicolor devices (`brightness`, `max_brightness`, `multi_index`, `multi_intensity`) are another route [V:K-ledsmc]. For example, `hid-steelseries` exposes MSI KLC (1038:1122) and ALC (1038:1161) through a feature report of 524 bytes, only on interface 0, only in master 7.3-rc. [V:K-steelseries]
  - An optional `leds` HAL backend could drive them; it needs a chgrp rule like §4.3 (brightnessctl uses group `input` for `leds`). [V:BCTL-rule]
- **Coexistence:**
  - OpenRGB can run as a root background service. [V:ORGB-udevmd] Its SDK's "default port … is 6742. This is "ORGB" on a telephone keypad". [V:ORGB-sdk] Detect a running OpenRGB by probing 127.0.0.1:6742, and if Lightloom ever offers an OpenRGB-SDK-compatible server, refuse to bind when the port is taken. [design]
  - hidraw has no exclusivity, so two applications' writes interleave. [V:K-hidraw-c]
  - usbfs interface claims are exclusive (`EBUSY`). [V:K-usb-driver]
  - i2c has only the per-transaction lock. [V:K-smbus-core]
  - The kernel `nzxt-kraken3` driver shares the NZXT device. [V:K-kraken3]
  - Report conflicts and never kill processes. Candidate process names (openrgb, openrazer-daemon, liquidctl, coolercontrold, ratbagd, solaar, ckb-next-daemon): [U].
- **systemd user unit:** review of `packaging/systemd/lightloomd.service` [V-repo]:
  - It is `Type=simple`; use `Type=notify` with `WatchdogSec=`.
  - Add `ExecStopPost=` for fan restore.
  - It uses `After=`, `PartOf=` and `WantedBy=graphical-session.target`. That target "is active whenever any graphical session is running" and is started and stopped by session targets such as gnome-session.target through `BindsTo=`. [V:SD-special] Consider `default.target` with `After=graphical-session.target`, and enable capture only when `WAYLAND_DISPLAY` or `DISPLAY` is set. [design]
  - With linger and no active seat session, uaccess ACLs do not apply, so the group rule is the fallback. [U]
- **Suspend handling:** [V:SD-login1][V:SD-logindconf][V:CR-zbus]
  - Call `org.freedesktop.login1.Manager.Inhibit(in s what, in s who, in s why, in s mode, out h pipe_fd)` with `("sleep", "Lightloom", "Save/restore device state", "delay")`.
  - On `PrepareForSleep(b start)` with `true`, run `Shutdown(true)` per plugin, then close the fd within `InhibitDelayMaxSec` (default 5).
  - On `false`, re-take the lock and run `Initialize()` again after about 3 s. [design]
  - Use zbus (MIT) for the D-Bus calls.

## 8. Crate and licence table (crates.io API, 2026-10-18)
| crate | version | licence | MSRV | native or other dependencies |
|---|---|---|---|---|
| hidapi | 2.6.7 (2026-08-27) | MIT | – | nix 0.30; `udev` 0.9 (libudev) **or** `basic-udev` 0.1; defaults compile the C hidapi |
| nusb | 0.2.8 (2026-10-14) | Apache-2.0 OR MIT | 1.85 | rustix 1.x and linux-raw-sys; no libusb |
| i2cdev | 0.6.2 (2025-11-11) | MIT/Apache-2.0 | – | nix 0.26.2 (a second nix version); no `I2C_FUNCS` call; `force_new` is `unsafe fn`; block writes silently truncate to 32 |
| udev | 0.9.3 | MIT | – | libudev-sys 0.1.4 (MIT), which links the system libudev (LGPL-2.1+); build needs `libudev-dev` |
| tokio-udev | 0.10.0 | Apache-2.0 OR MIT | – | udev 0.9, tokio |
| basic-udev | 0.1.2 | MIT OR Apache-2.0 | – | none (sysfs only, no monitor) |
| serialport | 4.10.1 | MPL-2.0 | 1.59.0 | default feature `libudev`. Without it, the sysfs scan still yields USB vid, pid, serial, manufacturer, product and interface |
| cpal | 0.18.2 | Apache-2.0 | 1.85 | **alsa 0.11 / alsa-sys 0.4.0 always on Linux** (`libasound2-dev`); optional `pipewire` 0.10 and `pulseaudio` 0.3 |
| pipewire / pipewire-sys | 0.10.1 / 0.10.1 | MIT | 1.80 | libpipewire-0.3 (MIT), bindgen 0.72 (libclang), system-deps 7; `links = "pipewire-0.3"` |
| pulseaudio | 0.3.2 (2026-10-16) | MIT | – | pure Rust |
| ashpd | 0.13.13 | MIT | **1.87** | zbus 5.13+ (`default-features = false`); optional pipewire 0.9 (never enable) |
| zbus | 6.0.0 (2026-10-16) / 5.19.0 | MIT | 1.89 / 1.87 | pure Rust |
| xcap | 0.9.8 | Apache-2.0 | – | Linux: xcb 1.7 (links libxcb, so `libxcb1-dev`), zbus ^5.17, pipewire 0.10, libwayshot-xcap 0.3.3 (BSD-2-Clause) |
| xcb | 1.7.1 | MIT | – | libxcb (link time) |
| alsa / alsa-sys | 0.11.0 / 0.4.0 | Apache-2.0/MIT / MIT | – | libasound (alsa-lib) through pkg-config `alsa` |
| sd-notify | 0.5.0 | MIT OR Apache-2.0 | 1.82 | – |
| nvml-wrapper | 0.13.0 | MIT OR Apache-2.0 | 1.60.0 | libloading 0.8, which loads `libnvidia-ml.so.1` at runtime |
| cef | 154.5.0+154.0.34 | Apache-2.0 OR MIT (CEF: BSD-3-Clause) | – | CEF branch 8037 (Legacy, unsupported); 152.4.0+152.0.8 and 150.2.1+150.0.14 lines also exist |
| i2c-linux (alternative) | 0.1.2 (2018-11-18) | MIT | – | stale |

[V:CR-* entries]. The repo `deny.toml` allow-list includes MIT, Apache-2.0, Apache-2.0 WITH LLVM-exception, BSD-2-Clause, BSD-3-Clause, ISC, Zlib, MPL-2.0 and others, so all of these pass. [V-repo]

## 9. Risks and open questions; tests to add
1. **Collection numbering and report-ID validation.**
   - SignalRGB's `collection` for 10 or more TLCs [U]. The repo already parses hex.
   - Whether Windows rejects foreign report IDs [U].
   - Whether Windows sends the caps length on the wire [U].
   - Capture on Windows during M2.
2. **Descriptor fixups** (driver `report_fixup` and HID-BPF) could change the TLC set. hid-generic devices without BPF are safe. Add per-device golden fixtures.
3. **The SMBus bus set differs from Windows.**
   - The extra PIIX4 port 2, ASF replacing aux on kernels 6.13 and later when `\_SB.ASFC` exists, the missing Nuvoton buses and the NVIDIA multi-port layout all change what `Scan()` sees.
   - i801 names vary by kernel.
   - NVIDIA port 1 for MSI RGB is OpenRGB evidence only. [U]
4. **SPD and kernel claims:** plugins touching 0x36/0x37 or 0x50-0x57 fail with `EBUSY` on kernels 6.11 and later (not on 24.04 GA 6.8). This is by design; surface a clear message.
5. **Fans:**
   - Fan control needs an out-of-tree GPL module, possibly ACPI `lax`, and possibly MOK signing [U]. These are user-performed steps.
   - The chmod rule must exclude mainline `nct6683` (unguarded `store_pwm`).
6. **CEF:**
   - The pinned branch 8037 is unsupported per CEF docs; pick 7977 (Extended), 7871 (LTC) or the next stable.
   - Headless is software-only; WebGL needs the "unsafe" SwiftShader flag; the sandbox fails on AppImage on Ubuntu 24.04 and later.
   - Linux `libcef` size is not measured [U].
7. **Security of uaccess on hidraw and i2c:** any process of the seat user can use these nodes. Narrow the rules (§1.6, §3.7).
8. **MSRV bump to 1.87.** Never enable both pipewire 0.9 and 0.10 in one build.
9. **CI on Ubuntu:**
   - Needed: `pkg-config`, `libclang-dev`, `libpipewire-0.3-dev`, `libasound2-dev` and `libxcb1-dev`. [V:UB-pkgs]
   - `libudev-dev` is needed if the `udev` crate or serialport's default `libudev` feature is used.
   - Remove `libusb-1.0-0-dev`.
   - Add a CEF headless smoke test with an `xvfb-run` fallback.
10. **Self-hosted Linux hardware run on this machine (dual boot):**
    - Validate-tuple fixtures.
    - PIIX4 port 0 Scan: Crucial remap 0x27 → 0x39/0x3A inside `TransactionLock`.
    - NVIDIA port-1 MSI GPU at 0x68.
    - Hybrid NZXT detach and reattach.
    - usbmon captures for HID wire parity [U procedure].
11. **Ubuntu targets:** 24.04 has kernel 6.8, systemd 255.4 and PipeWire 1.0.4 [V:UB-2404]. 26.04 has kernel 7.0, systemd 259 and rustc 1.93 [V:UB-2604]. Behaviour differs per §0.1, so test both.
12. **Flatpak hotplug** needs `--share=network`; plan for an inotify fallback [U].

## 10. Source keys
- **Kernel** (`https://raw.githubusercontent.com/torvalds/linux/master/` + path; per-version checks use `/vX.Y/` = **K-tags**):
  - Build and HID: K-make `Makefile`; K-hidraw-h `include/uapi/linux/hidraw.h`; K-hidraw-rst `Documentation/hid/hidraw.rst`; K-hidraw-c `drivers/hid/hidraw.c`; K-hidcore `drivers/hid/hid-core.c`; K-usbhid `drivers/hid/usbhid/hid-core.c`; K-hid-h `include/linux/hid.h` + `include/uapi/linux/hid.h`; K-input-h `include/uapi/linux/input.h`; K-usb-h `include/linux/usb.h`; K-usb-sysfs `drivers/usb/core/sysfs.c`; K-usb-driver `drivers/usb/core/driver.c`.
  - I2C core: K-i2cdev-h `include/uapi/linux/i2c-dev.h`; K-i2c-h `include/uapi/linux/i2c.h`; K-i2cdev-c `drivers/i2c/i2c-dev.c`; K-smbus-core `drivers/i2c/i2c-core-smbus.c`; K-i2c-core-base `drivers/i2c/i2c-core-base.c`; K-i2c-smbus `drivers/i2c/i2c-smbus.c`.
  - I2C bus drivers: K-piix4 `drivers/i2c/busses/i2c-piix4.c`; K-i801 `.../i2c-i801.c`; K-asf `.../i2c-amd-asf-plat.c`; K-busses-mk `.../Makefile`; K-busses-kconfig `.../Kconfig`; K-nvidia-gpu `.../i2c-nvidia-gpu.c`; K-ee1004 `drivers/nvmem/ee1004.c`; K-nvmem-core `drivers/nvmem/core.c`.
  - Docs and parameters: K-devif `Documentation/i2c/dev-interface.rst`; K-params `Documentation/admin-guide/kernel-parameters.txt`.
  - hwmon: K-nct6683-c `drivers/hwmon/nct6683.c`; K-nct6683-rst `Documentation/hwmon/nct6683.rst`; K-nct6775-rst `Documentation/hwmon/nct6775.rst`; K-hwmon-abi `Documentation/ABI/testing/sysfs-class-hwmon`; K-k10temp `drivers/hwmon/k10temp.c`; K-kraken3 `drivers/hwmon/nzxt-kraken3.c`; K-kraken3-rst `Documentation/hwmon/nzxt-kraken3.rst`.
  - HID drivers: K-steelseries `drivers/hid/hid-steelseries.c`; K-hid-ids `drivers/hid/hid-ids.h`; K-hid-quirks `drivers/hid/hid-quirks.c`; K-hid-razer `drivers/hid/hid-razer.c`; K-hid-evision `drivers/hid/hid-evision.c`.
  - Lighting: K-tuxedo `drivers/platform/x86/tuxedo/nb04/wmi_ab.c`; K-ledsmc `Documentation/leds/leds-class-multicolor.rst`.
  - USB, memory and misc: K-usbfs-h `include/uapi/linux/usbdevice_fs.h`; K-devio `drivers/usb/core/devio.c`; K-mem `drivers/char/mem.c`; K-devtmpfs `drivers/base/devtmpfs.c`; K-pci-sysfs `drivers/pci/pci-sysfs.c`; K-pci-ids `include/linux/pci_ids.h`; K-af-netlink `net/netlink/af_netlink.c`; K-sysfs-file `fs/sysfs/file.c`.
  - GPU: K-amdgpu-conn `drivers/gpu/drm/amd/display/amdgpu_dm/amdgpu_dm_connector.c`; K-navi10 `drivers/gpu/drm/amd/pm/swsmu/smu11/navi10_ppt.c`.
  - K-511: https://github.com/torvalds/linux/commit/105856b36c0cefc2fa1c1e649d75da71e2e38c31.patch (HID merge containing Dean Camera's two 5.11 patches).
- **NVIDIA** (https://raw.githubusercontent.com/NVIDIA/open-gpu-kernel-modules/main/): NV-i2c `kernel-open/nvidia/nv-i2c.c`; NV-nv.c `kernel-open/nvidia/nv.c`; NV-readme `README.md`; NV-copying `COPYING`; plus `version.mk` (615.78.08).
- **systemd** (https://raw.githubusercontent.com/systemd/systemd/main/):
  - Rules and hwdb: SD-uaccess `rules.d/70-uaccess.rules.in`; SD-seatlate `rules.d/73-seat-late.rules.in`; SD-default `rules.d/50-udev-default.rules.in`; SD-persist-hidraw `rules.d/60-persistent-hidraw.rules`; SD-80drivers `rules.d/80-drivers.rules`; SD-lights `hwdb.d/70-lights.hwdb`.
  - Other files: SD-NEWS `NEWS`; SD-lic `LICENSES/README.md`; SD-mon `src/libsystemd/sd-device/device-monitor.c` + `device-monitor-private.h`; SD-basic `sysusers.d/basic.conf.in`; SD-ver `meson.version`.
  - Man pages (https://www.freedesktop.org/software/systemd/man/latest/): SD-login1 `org.freedesktop.login1.html`, SD-logindconf `logind.conf.html`, SD-special `systemd.special.html`, SD-service `systemd.service.html`, SD-sysusers `sysusers.d.html`, SD-modload `modules-load.d.html`.
- **Crates:**
  - CR-* = https://crates.io/api/v1/crates/<name> plus https://static.crates.io/crates/<name>/<name>-<ver>.crate.
  - Covered: hidapi 2.6.7, nusb 0.2.8, i2cdev 0.6.2, udev 0.9.3, libudev-sys 0.1.4, tokio-udev 0.10.0, basic-udev 0.1.2, serialport 4.10.1, cpal 0.18.2, pipewire 0.10.1 (CR-pw), pipewire-sys 0.10.1 and 0.9.2 (CR-pwsys), pipewire 0.9.2, pulseaudio 0.3.2, ashpd 0.13.13, zbus, xcap 0.9.8, xcb 1.7.1, alsa 0.11.0 and alsa-sys 0.4.0 (CR-alsa), libwayshot-xcap 0.3.3 (CR-wayshot), sd-notify 0.5.0, nvml-wrapper 0.13.0 (CR-nvml), cef 154.5.0+154.0.34, i2c-linux 0.1.2.
- **CEF and Chromium:**
  - CEF-index https://cef-builds.spotifycdn.com/index.json.
  - CEF repo files (https://raw.githubusercontent.com/chromiumembedded/cef/master/): CEF-redistrib `tools/distrib/linux/README.redistrib.txt`; CEF-usage `docs/general_usage.md`; CEF-branches `docs/branches_and_building.md`; CEF-cmake `cmake/cef_variables.cmake.in`.
  - CEF issues: CEF-3902 and CEF-3953 via api.github.com/repos/chromiumembedded/cef/issues/<n> (+ /comments).
  - Forum: CEF-forum1 https://magpcss.org/ceforum/viewtopic.php?t=17335; CEF-forum2 https://magpcss.org/ceforum/viewtopic.php?p=42879&t=16993.
  - CEFRS-readme https://raw.githubusercontent.com/tauri-apps/cef-rs/dev/README.md.
  - Chromium (https://chromium.googlesource.com/chromium/src/+/main/<path>?format=TEXT): CH-sandbox `sandbox/linux/README.md`; CH-suid `docs/linux/suid_sandbox_development.md`; CH-swiftshader `docs/gpu/swiftshader.md`; CH-ozone `docs/ozone_overview.md`; CH-debdeps `chrome/installer/linux/debian/dist_package_versions.json`; CH-adddeps `chrome/installer/linux/debian/additional_deps`.
  - CH-blinkdev https://groups.google.com/a/chromium.org/g/blink-dev/c/yhFguWS_3pM/m/oLrB5up_BwAJ.
- **Distro and packaging:**
  - Ubuntu: UB-2404 https://documentation.ubuntu.com/release-notes/24.04/; UB-2604 https://documentation.ubuntu.com/release-notes/26.04/summary-for-lts-users/; UB-blog https://ubuntu.com/blog/ubuntu-23-10-restricted-unprivileged-user-namespaces; UB-pkgs https://packages.ubuntu.com/noble/<pkg>.
  - AppImage and zypak: AI-docs https://docs.appimage.org/user-guide/troubleshooting/electron-sandboxing.html; ZYPAK https://raw.githubusercontent.com/refi64/zypak/main/README.md + LICENSE.
  - Flatpak (https://raw.githubusercontent.com/flatpak/flatpak/main/common/): FP-run `flatpak-run.c`; FP-ctx `flatpak-context.c` (+ `flatpak-run-pulseaudio.c`).
  - Portals: XDP-sc https://raw.githubusercontent.com/flatpak/xdg-desktop-portal/main/data/org.freedesktop.portal.ScreenCast.xml; XDP-usb `.../data/org.freedesktop.portal.Usb.xml`; XDP-list https://api.github.com/repos/flatpak/xdg-desktop-portal/contents/data.
  - Others: PW-lic https://gitlab.freedesktop.org/pipewire/pipewire/-/raw/master/LICENSE; DEB-i2ct https://sources.debian.org/data/main/i/i2c-tools/{4.4-3,4.3-2}/debian/{i2c-tools.udev,i2c-tools.postinst,i2c-tools.sysusers}; MM-NEWS https://gitlab.freedesktop.org/mobile-broadband/ModemManager/-/raw/main/NEWS; MM-filter https://distributions.freedesktop.org/software/ModemManager/api/latest/ref-overview-modem-filter.html + ch03s02.html.
- **Other:**
  - nct6687d: NCT-readme https://raw.githubusercontent.com/Fred78290/nct6687d/main/README.md; NCT-repo https://api.github.com/repos/Fred78290/nct6687d (+ commits?path=README.md).
  - OpenRGB: ORGB-* = GitLab API https://gitlab.com/api/v4/projects/CalcProgrammer1%2FOpenRGB/repository/files/<path>/raw?ref=master.
    - Files read: Documentation/SMBusAccess.md (ORGB-smbus), Documentation/KernelParameters.md, Documentation/UdevRules.md (ORGB-udevmd), Documentation/OpenRGBSDK.md (ORGB-sdk), DetectionManager.cpp (ORGB-detect), i2c_smbus/Linux/i2c_smbus_linux.cpp (ORGB-i2clinux), i2c_smbus/i2c_amd_gpu.h (ORGB-amdgpu), Controllers/MSIGPUController/MSIGPUController/MSIGPUControllerDetect.cpp (ORGB-msigpu), pci_ids/pci_ids.h (ORGB-pciids), Controllers/ENESMBusController/ENESMBusInterface/ENESMBusInterface_i2c_smbus.cpp (ORGB-ene).
  - Udev precedents: DDC-rule https://raw.githubusercontent.com/rockowitz/ddcutil/2.2.3-release/data/usr/lib/udev/rules.d/60-ddcutil-i2c.rules; BCTL-rule https://raw.githubusercontent.com/Hummer12007/brightnessctl/master/90-brightnessctl.rules.
  - I2CT-detect https://git.kernel.org/pub/scm/utils/i2c-tools/i2c-tools.git/plain/tools/i2cdetect.c (primary; replaces the mozilla-b2g mirror).
  - MS-hwid https://learn.microsoft.com/en-us/windows-hardware/drivers/hid/hidclass-hardware-ids-for-top-level-collections; PHX-oem https://www.phoronix.com/news/AMDGPU-More-i2c-Bus.
- **Repo files cited** (`%USERPROFILE%\Nextcloud\Archivio\Progetti\lightloom\`):
  - Code: `crates/lightloom-hal-windows/src/path.rs` (mtime 2026-10-18 11:06), `crates/lightloom-hal-windows/src/hid.rs`, `crates/lightloom-hal/src/hid.rs`, `crates/lightloom-hal/src/smbus.rs`, `crates/lightloom-hal-linux/src/lib.rs` and `Cargo.toml`.
  - Packaging and tools: `packaging/udev/70-lightloom.rules.template`, `packaging/systemd/lightloomd.service`, `packaging/README.md`, `tools/gen-udev/README.md`.
  - Docs: `docs/host-api/smbus.md`, `docs/host-api/macros-system.md`, `docs/architecture.md`.
  - Workspace and CI: `Cargo.toml`, `Cargo.lock`, `deny.toml`, `.github/workflows/ci.yml`.
  - Plan: `%USERPROFILE%\.claude\plans\use-rea-to-find-serialized-salamander.md` (V-plan).

---

## Fact-check corrections applied to the first draft

- Top finding #3 is obsolete. crates/lightloom-hal-windows/src/path.rs (mtime 2026-10-18 11:06) already parses &colNN as hex (u16::from_str_radix(h,16)) and documents 'never decimal'. Only SignalRGB's collection value for 10 or more TLCs and the 'Col%02X' hidclass claim remain unverified.
- hidraw: HIDIOCSOUTPUT does not go to the control endpoint. hidraw_send_report(HID_OUTPUT_REPORT) tries __hid_hw_output_report (interrupt OUT) first, exactly like write(), and falls back to SET_REPORT only on -ENOSYS. HID_QUIRK_NO_OUTPUT_REPORTS_ON_INTR_EP forces SET_REPORT.
- The brief said the 5.11 patch 'fixed the docs: feature reads include the report ID'. That is only partly true: hidraw.rst still says data begins at byte 0 for unnumbered reports, while the usbhid code keeps buf[0]=0 and puts the data at buf[1].
- Added the exact ioctl values: HIDIOCREVOKE 0x4004480D (present since v6.12, absent in v6.11; argument must be 0), HIDIOCGRAWNAME/PHYS/UNIQ 0x80004804/05/08, HIDIOCS/GINPUT and S/GOUTPUT 0xC0004809-0C, and all usbfs ioctl values (for example DISCONNECT_CLAIM 0x8108551B and the newly listed DROP_PRIVILEGES 0x4004551E). After HIDIOCREVOKE, read/write/ioctl return ENODEV and poll reports EPOLLERR|EPOLLHUP.
- Upgraded from U to V: the HID_PHYS prefix is 'usb-<bus_name>-<devpath>/input<N>' (usb_make_path snprintf 'usb-%s-%s'); the sysfs report_descriptor and HIDIOCGRDESC both return the post-HID-BPF, post-report_fixup rdesc; bInterfaceNumber is printed '%02x' (drivers/usb/core/sysfs.c).
- Added: usbhid_probe returns -ENODEV for a HID interface without an interrupt-IN endpoint, so such an interface gets no hidraw node. HIDRAW_MAX_DEVICES=64 is a system-wide minor pool, and hidraw_connect fails with -EINVAL when it is exhausted.
- Timeout nuance: usbhid uses USB_CTRL_SET_TIMEOUT for GET_REPORT, SET_REPORT and interrupt-OUT. Both timeout constants are 5000 ms.
- hidapi 2.6.7 details added. The default features are linux-static-hidraw and illumos-static-libusb (the C build needs libudev). HidDevice::get_input_report is compiled only with cfg(any(hidapi, target_os='linux')), so windows-native lacks it. The Windows TODO is in crates/lightloom-hal-windows/src/hid.rs L188-193, not in lightloom-hal/src/hid.rs.
- udev template nuance: its rules have no ACTION filter, so they already match change events. The real problem is breadth, but the systemd-recommended ACTION!="remove" form should still be adopted.
- i2c group gap filled: Debian i2c-tools creates the system group 'i2c' (postinst addgroup in 4.3-2; sysusers 'g i2c' in 4.4-3) and ships SUBSYSTEM=="i2c-dev",KERNEL=="i2c-[0-9]*", GROUP="i2c", MODE="0660". systemd basic.conf.in has no i2c or plugdev group.
- ModemManager: ID_MM_DEVICE_IGNORE is honoured 'regardless of what filter type is in use (already in 1.10.6)', per the NEWS entry in the 1.12.0 section. The brief said 1.10.8.
- systemd 257 hidraw handoff upgraded from U to V: TakeDevice 'allows a session controller to get a file descriptor' and 'only requires you to be the active session controller (see TakeControl())'. Each device can be requested only once.
- Added: systemd 258 and later creates /dev/input/by-id/*-hidraw and /dev/input/by-path/*-hidraw symlinks (60-persistent-hidraw.rules). These help stable naming on Ubuntu 26.04 (systemd 259) but are absent on 24.04.
- Added: inside Flatpak, udev hotplug needs --share=network. Netlink multicast is netns-scoped (af_netlink do_one_broadcast net_eq), and flatpak-run adds --unshare-net when network is not shared.
- nusb/usbfs upgraded from U to V: usb_driver_claim_interface returns -EBUSY when the interface has a bound driver, which also makes usbfs claims exclusive across processes. DISCONNECT_CLAIM with EXCEPT_DRIVER "usbfs" returns EBUSY instead of stealing. Added: the USB portal is documented as version 1, and Flatpak has a --usb=VID:PID option.
- PIIX4: verified the aux base derivation (aux = main | 0x20 for KERNCZ rev >= 0x49), consistent with SignalRGB's b00/b20 logs. SPD auto-instantiation via piix4 exists only since v6.11 (none in v6.10/6.8), so it does not apply on Ubuntu 24.04 GA 6.8. The i2c_register_spd_write_enable name dates from v6.16. Master clamps slots to 8 (min(slot_count,8)) rather than requiring at most 8; 6.8 bailed out above 8.
- AMD ASF adapter: amd_asf_xfer rejects all reads ('ASF: Read not supported', EOPNOTSUPP) and sends writes as block writes with PEC, so it is effectively write-only. The matrix entries 'ReadByteWithoutRegister ✓' and 'WriteBlock ✓' were wrong; exclude this adapter. ASF first shipped in v6.13 (upgraded from U to V).
- i801 adapter names differ by kernel: 'SMBus I801 adapter at %04lx' (6.11 and earlier, including 6.8), 'SMBus %s adapter at %04lx' (6.12-6.14), and 'SMBus %s adapter at <pci_name>' (6.15 and later). The brief gave only the newest form.
- Upgraded from U to V: plain read()/write() and I2C_RDWR on PIIX4 fail with EOPNOTSUPP, because __i2c_transfer has no master_xfer ('I2C level transfers not supported').
- ee1004 compat path upgraded from U to V: /sys/bus/i2c/devices/<bus>-0050/eeprom, mode 0444, created only when CONFIG_NVMEM_SYSFS is set. The ee1004 source is drivers/misc/eeprom/ee1004.c through v7.2 and drivers/nvmem/ee1004.c only in master (7.3-rc).
- I2C_SLAVE nuance: it returns EBUSY only when a client at the address has dev->driver bound (i2cdev_check). An instantiated but unbound client does not block it.
- PIIX4 IMC nuance: IMC notification happens only for I2C_SMBUS_BLOCK_DATA when notify_imc is set (KERNCZ with an active IMC). It is not done on every block transaction.
- Added the exact i2cdev 0.6.2 API (new, unsafe force_new, the smbus_* methods, LinuxI2CBus + I2CTransfer). smbus_write_block_data silently truncates to 32 bytes. smbus_write_quick(false) maps to I2C_SMBUS_WRITE, which is how SignalRGB WriteQuick should be implemented.
- Verified the PCI sysfs formats ('0x%04x' and class '0x%06x') and PCI_CLASS_SERIAL_SMBUS 0x0c05. The udev rule should match ATTRS{class}=="0x0c05*" rather than '0x0c0500', because the prog-if byte is unverified.
- i2c-piix4 autoload mechanism verified: MODULE_DEVICE_TABLE(pci) includes 1022:790b, and systemd 80-drivers.rules runs 'kmod load' on MODALIAS. Whether Ubuntu builds it as a module stays unverified.
- Upgraded from U to V: i2c-nvidia-gpu is 'used to communicate with the GPU's Type-C controller' (Kconfig; it selects I2C_CCGX_UCSI). Also verified that i2c-nct6694 is 'a USB to I2C interface' and i2c-npcm7xx is a BMC controller.
- NVIDIA additions: the open-modules README says the code base is shared with the proprietary drivers. The kernel module is dual MIT/GPLv2. There is an 'NVIDIA SOC i2c adapter %u\n' variant. OpenRGB's table lists 10DE:2504 + 1462:3903 as 'MSI GeForce RTX 3060 Gaming X Trio LHR' at 0x68. The 'no hwmon' check covered only nv.c.
- amdgpu: the OEM bus was verified by source diff to be absent in v6.14 and present in v6.15. Its name now lives in amdgpu_dm_connector.c; in 6.15 it was in amdgpu_dm.c.
- Safety correction: mainline nct6683 restricts pwm writes to Mitac only through the file mode (is_visible), and store_pwm has no guard. The brief's proposed chmod g+w rule keyed only on ATTR{name}=="nct6687" would also unlock writes on the mainline driver, which uses the same hwmon name. The rule must add DRIVERS=="nct6687" (mainline's platform driver/device is named 'nct6683').
- nct6687d: the README was last changed 2026-09-08; the repository was last pushed 2026-10-13. The brief said the README was updated 2026-10-13. Added fan_mask (default 0xff) and temp_mask (default 0x7f). The blacklist of nct6683 is stated as required for msi_fan_brute_force.
- k10temp: the Ryzen 5 5600X (family 19h, model 0x21) exposes Tctl and Tccd1. Tdie is shown only for tctl_offset_table CPUs (Ryzen 1600X/1700X/1800X/2700X and Threadripper 19xx/29xx). The brief listed Tdie.
- nzxt-kraken3 attribute names upgraded from U to V: fan1_input (pump RPM), temp1_input (coolant), pwm1 and pwm1_enable (0/1/2), and temp[1-2]_auto_point[1-40]_pwm. The hwmon name for 1E71:2007 is 'x53'. fan2/pwm2 exist only on Z-series and Kraken 2023.
- nvml-wrapper upgraded from U to V: it loads libnvidia-ml.so.1 at runtime through libloading 0.8.
- sd-notify 0.5.0 has MSRV 1.82 (the brief said '–'). Its API is notify(&[NotifyState]) and watchdog_enabled().
- CEF: branch 8037 (M154), pinned by the repo, is listed under 'Legacy Release Branches (Unsupported)' in docs/branches_and_building.md. The supported branches are 8078/M156 beta, 7977/M152 extended (last refresh Oct 20, 2026) and 7871/M150 LTC (until Apr 13, 2027). The cef crate also has 152.x and 150.x lines.
- CEF forum2 (t=16993) statements are by 'Czarek', not by the CEF maintainer. The maintainer quotes come from forum1 (2020-01-21, 2020-01-22 and 2022-03-31).
- Ubuntu AppArmor example: the 24.04 release-notes profile (/etc/apparmor.d/chrome, flags=(unconfined)) is correct as quoted. The 23.10 blog uses a different path (/etc/apparmor.d/opt.google.chrome.chrome) and flags=(default_allow), so the brief's combined attribution was imprecise.
- Chromium dist_package_versions.json covers only Debian 11/12 and Ubuntu 18.04/20.04/22.04, not 24.04. Ubuntu 24.04 package libasound2t64 (1.2.11) exists, verified on packages.ubuntu.com.
- SwiftShader: deprecation began in Chrome 130, and the blink-dev 'Intent to Remove: SwiftShader Fallback' (2025-02-13) targets 'Shipping on Desktop 137'. This is more precise than 'removed in recent builds'.
- cef-rs: the linux-x11 feature does nothing in library code. The bundler forwards '-F linux-x11' to the app's cargo build. The brief said it 'only affects the bundler's own build'.
- Added: on Linux, cpal 0.18.2 always depends on alsa 0.11, which needs alsa-sys 0.4.0 (links 'alsa'; pkg-config alsa). libasound2-dev is therefore a build dependency even with the pipewire host.
- pipewire 0.10 connect API upgraded from U to V: Context::connect_fd(&self, fd: OwnedFd, properties: Option<PropertiesBox>) -> Result<CoreBox<'_>, Error>, with ContextRc::connect_fd_rc as the Rc variant.
- Added: ScreenCast v6 deprecates the stream node ID for targeting in favour of the 'pipewire-serial' property and PW_KEY_TARGET_OBJECT. ashpd 0.13.13's Stream does not expose the serial.
- ashpd set_sources takes impl Into<Option<BitFlags<SourceType>>>. The example now uses BitFlags::from(SourceType::Monitor); whether a bare enum compiles is unverified.
- xcap CI packages upgraded from U to V: xcb 1.7.1 links only libxcb ('xcb'), unless the dl feature is enabled, so libxcb1-dev is needed. libxcb-randr0-dev is not required.
- The brief's open claim that 'serialport without libudev loses USB VID/PID' is false. Without the feature, the /sys/class/tty scan still reads idVendor, idProduct, serial, manufacturer, product and bInterfaceNumber.
- CI: libudev-dev is also needed for serialport's default 'libudev' feature (the workspace pins serialport = "4.10" with defaults), not only for the udev crate. Added libasound2-dev, libpipewire-0.3-dev, libclang-dev and libxcb1-dev.
- Flatpak: there is no audio portal (verified against the xdg-desktop-portal data/ interface listing) and no 'pipewire' socket permission (flatpak_context_sockets). --device=input binds only /dev/input ('raw and virtual input currently require --device=all').
- Repo attribution corrected: the 'Flatpak post-install' udev step is in tools/gen-udev/README.md L41, not in packaging/README.md. 'Audio via portals' is in packaging/README.md.
- Ubuntu 26.04 kernel 7.0, systemd 259 and rustc 1.93 upgraded from U to V, using Ubuntu's own 26.04 'summary for LTS users' instead of the secondary source.
- Version facts added: the Tuxedo NB04 virtual LampArray exists since 6.16. hid-steelseries MSI KLC/ALC support exists only in master 7.3-rc and is absent in v7.2. nct6683 MSI IDs 0x207/0x20d are present in 7.0 but not in 6.8.
- i2cdetect source replaced: the kernel.org i2c-tools git is now the primary source instead of the mozilla-b2g mirror. The content (UU on EBUSY; Receive Byte on 0x30-0x37/0x50-0x5F) is confirmed.
- OpenRGB SDK port 6742 verified ('This is "ORGB" on a telephone keypad'). Reworded the brief's ambiguous 'refuse to bind if that port is taken' into a detect-or-refuse design note.
- Verified that zypak's licence includes the 'Neither the name' clause (BSD-3-Clause) and that zypak requires Platform 21.08 or later.
- PipeWire licence nuance: libpipewire is MIT, but libspa-alsa.so is LGPL and libjackserver.so is GPL2.
- Process note, not a correction to the brief: the verifier's first batch of crates.io API requests sent a User-Agent header containing the user's email address. All later requests used a generic User-Agent. Tell the user about this.

## Still unverified after the fact-check

- Windows HIDClass numbers top-level collections (ColNN) in report-descriptor order. Only repo M2 evidence supports this.
- SignalRGB's endpoint.collection value for devices with 10 or more TLCs, and the repo's claim that hidclass.sys formats the field as 'Col%02X'.
- Windows HIDClass rejects output/feature report IDs not declared in the opened TLC.
- The exact on-wire SET_REPORT/WriteFile length on Windows: caps length versus per-report-ID length.
- That multi-TLC interfaces always use report IDs (HID-spec reasoning, not re-read).
- That usb_device sysfs string attributes (manufacturer/product/serial) are absent when the string descriptor is missing.
- That Cargo resolver 2 keeps the cfg(windows) 'windows-native' hidapi feature out of Linux builds (Cargo semantics, not re-read).
- MSI B550 Tomahawk: the DIMM RGB controllers sit on 'SMBus PIIX4 adapter port 0 at 0b00'; the BIOS has no \_SB.ASFC, so the aux adapter at 0x0b20 is registered on kernels 6.13 and later; the BIOS has no ACPI resource conflict for the PIIX4 SMBus or the NCT6687D EC region (0xA24-0xA27).
- The NVIDIA Linux driver registers I2C adapters for the RTX 3060 by default, and the MSI RGB controller at 0x68 responds on NVIDIA port 1. Only the OpenRGB detector table is evidence.
- That proprietary (non-open) NVIDIA kernel modules use identical adapter naming. Only the README statement that the code base is shared supports this. That the whole NVIDIA module (not just nv.c) has no hwmon.
- The PCI prog-if byte of the AMD FCH SMBus function (sysfs class 0x0c05xx), so the exact string 0x0c0500 is unconfirmed.
- That Ubuntu kernels build i2c-piix4 and ee1004 as autoloadable modules.
- Secure Boot requires MOK signing for the DKMS nct6687d build (the README does not say).
- The sysfs modes of the out-of-tree nct6687 pwm attributes (assumed 0644), and that udev DRIVERS=="nct6687" matches at hwmon add time.
- Whether AMDGPU DM i2c supports the zero-length message needed for emulated SMBus QUICK.
- Severity of the risk that user-level SMBus access corrupts DDR4 SPD contents.
- libcef.so in the CEF Linux minimal distribution is unstripped; the stripped size and the resulting AppImage/Flatpak sizes are unmeasured.
- libcef.so's DT_NEEDED set matches Chrome's Debian dependency list.
- CEF 154 off-screen rendering works reliably with --ozone-platform=headless. Evidence is an M133 issue report; the maintainer's 2020/2022 statements say a WM backend is required.
- An AppArmor profile cannot target an AppImage's random /tmp/.mount_* path, so the userns sandbox cannot be allowed for AppImages on Ubuntu 24.04 and later.
- An AppArmor profile with flags=(unconfined) and 'userns,' works unchanged for /opt/lightloom/lightloom-renderer.
- Whether a given Flatpak runtime's libudev contains the user-namespace sender-uid check, and whether an inotify-on-/dev fallback works with --device=all.
- Audio monitor capture inside Flatpak through --socket=pulseaudio works in practice, and the --filesystem=xdg-run/pipewire-0 workaround for native PipeWire.
- The Flatpak Background portal (ashpd 'background' feature) behaves as needed to autostart a daemon.
- Minimum xdg-desktop-portal and Flatpak versions providing the USB portal and --usb=VID:PID.
- Requesting memfd/SHM buffers in a PipeWire screencast stream avoids DMA-BUF import.
- The cpal PulseAudio host captures sink monitors through a '<default sink>.monitor' source name.
- Whether ashpd SelectSourcesOptions::set_sources accepts a bare SourceType::Monitor, or needs BitFlags::from(..).
- Linux process names to report as conflicting software (openrgb, openrazer-daemon, liquidctl, coolercontrold, ratbagd, solaar, ckb-next-daemon).
- The interaction of 'loginctl enable-linger' with uaccess (no ACLs without an active seat session).
- Per-distro existence of the plugdev group, and whether Ubuntu's i2c-tools package carries Debian's i2c group and udev rule.
- The usbmon-based capture procedure for HID wire-parity fixtures.
- Which HWE kernels Ubuntu 24.04.x point releases ship (6.11 or later would enable piix4 SPD instantiation and the other changes in the version matrix).
