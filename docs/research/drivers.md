<!-- SPDX-License-Identifier: GPL-3.0-or-later -->

# Driver audit: what "works out of the box on Windows" needs

Status: research, 2026-10-22, judged against develop `bfc3190`. Tags: **[V]** verified
against a cited source, **[V-local]** verified on this machine or in this repo, **[U]**
unverified or inferred.

Question: which device classes need anything that a stock Windows 10/11 install plus the
vendor's GPU driver does not already provide, and what HueDeck should do about each.

## 0. Summary

| Device class | Extra driver needed? | HueDeck today | Recommendation |
|---|---|---|---|
| HID devices (keyboards, mice, headsets, MSI/NZXT/Razer/SteelSeries controllers): about 90 % of plugins | **No**. `hid.dll` + inbox `hidclass.sys`/`hidusb.sys`. | Works (hidapi `windows-native`). | Nothing to do. |
| HID LampArray (Dynamic Lighting) | No (inbox). | Collection 0x59/0x01 skipped; native backend not built (M6). | See §6 for the conflict with Windows Dynamic Lighting. |
| Raw USB (`Type() == "rawusb"`/`"hybrid"`: Lian Li Uni Fan SL/AL/original, Aqua Computer, Thermalright/Turzx LCDs, Lian Li Universal Screen, Asus Ryujin and NZXT Kraken LCD streams) | **Depends on the device.** Yes for vendor-class interfaces without MS OS descriptors. | **Not implemented.** `UsbBackend` is a trait with no Windows implementation; the plugin host has no `bulk_transfer`/`control_transfer`; `catalog.rs` refuses rawusb/hybrid plugins. | Implement with `nusb` (WinUSB) for devices that self-bind WinUSB. Do **not** ship an INF yet. §2. |
| Serial (7 corpus plugins) | CDC-ACM: no (inbox `usbser.sys`, Windows 10+). FTDI/CP210x/CH340: the vendor VCP driver, usually delivered by Windows Update [U]. | Not implemented. | Document it; no HueDeck action. |
| NVIDIA GPU RGB (I2C over NvAPI) | No. `nvapi64.dll` ships with the NVIDIA driver. | Done (M5, loaded dynamically). | Nothing to do. |
| AMD GPU RGB (I2C over ADL) | **No.** `atiadlxx.dll` ships with the AMD driver. | Done (WP-AMDGPU, `crates/huedeck-adl`; mock-tested only). | Hardware test on an AMD card. §3. |
| AMD chipset SMBus (RAM, some boards) | PawnIO (opt-in, signed third-party driver). | Done: broker + `SmbusPIIX4` pinned. | Keep as is. |
| Intel chipset SMBus (RAM) | PawnIO + `SmbusI801`. | Missing (not pinned, no backend). | Pin `SmbusI801` exactly like PIIX4. §4. |
| SuperIO/LPC (NCT6687D fans/LEDs, ITE) | PawnIO + `LpcIO`. | Missing. | Pin `LpcIO` only when a feature uses it, behind closed high-level ops. §5. |
| Runtime DLLs of our own exes | **Yes: `VCRUNTIME140.dll`** (not part of Windows). | Not shipped and not statically linked. | Build with `+crt-static`. §7. |
| CEF | No. `libcef.dll` and friends import only system DLLs. | Shipped. | Nothing to do (note on Windows N editions). |

Bottom line: on a clean PC **the only hard blocker today is `VCRUNTIME140.dll`**. Every HID
device already works driver-free. The remaining driver questions all concern opt-in
features: raw USB, Intel/AMD-GPU SMBus, and SuperIO.

## 1. Evidence from this repository

- `crates/huedeck-hal/src/usb.rs` defines `UsbBackend`/`UsbHandle` (`bulk_transfer`,
  `control_transfer`, and `driver_bound` = "Whether a user-mode-accessible driver (WinUSB/libusb) is bound").
  No crate implements it [V-local].
- `crates/huedeck-hal-windows/src/lib.rs:37`: "Raw USB (`nusb`), serial and LampArray
  follow in later milestones". `crates/huedeck-hal-windows/Cargo.toml:24`:
  `TODO(M2): ... nusb, serialport`. `nusb = "0.2"` is a workspace dependency, but no crate uses it [V-local].
- `crates/huedeck-plugin-host/src/catalog.rs:375`: "Hybrid plugins also need libusb
  bulk/control transfers, which device sessions do not provide yet: refusing them beats
  faking that I/O." No first-party plugin uses rawusb, hybrid, `bulk_transfer`,
  `control_transfer` or `setProtocol` [V-local].
- ADR-0007 and `docs/architecture.md` say `huedeck-setup` binds WinUSB to user-approved
  VID:PIDs. `crates/huedeck-setup` has no WinUSB/INF code [V-local]. This document
  recommends dropping that promise for now (§2.4).
- Every SignalRGB-shaped test device on this PC is plain HID: MSI 1462:7C91, Razer 1532:007C/007D/007E,
  NZXT 1E71:2007, SteelSeries 1038:12AA and Sonix 320F:5064. Win32_PnPSignedDriver shows **no** device bound to
  WinUSB, libusbK or UsbDk on this machine (read-only CIM query) [V-local].

## 2. Raw USB / WinUSB

### 2.1 Who needs it
SignalRGB corpus (`docs/host-api/hid-usb.md` §1, §4): `Type()` is `rawusb` in 18 files and `hybrid` in 11.
- **rawusb**: Lian Li Uni Fan (original 0CF2:7750) and Uni Fan SL 1.2 (0CF2:A100/A106), which use only vendor control
  transfers (0x40/0x80 OUT, 0xC0/0x81 IN). Aqua Computer D5 Next, Farbwerk, Octo and Quadro use `bulk_transfer(0x02, 364 B)`.
  Thermalright LCD and Turzx Screen 8.8 use bulk 0x01/0x81. Lian Li Universal Screen 8.8 uses bulk with a DES header and JPEG.
- **hybrid** (HID + libusb on the same device): Lian Li SL V2 / AL V2 / AL 1.1 (HID writes plus control transfers for RPM),
  Asus Ryujin III (LCD frames over bulk 0x01), and NZXT Kraken Z/Elite LCD (`setProtocol("hybrid")`).

OpenRGB d4798c12 (read-only clone) uses libusb on Windows for:

| Family | VID:PID | libusb backend on Windows | Source (licence) |
|---|---|---|---|
| Lian Li Uni Hub (original) | 0CF2:7750 | default (WinUSB) | `Controllers/LianLiController/LianLiControllerDetect.cpp` (GPL-2.0-or-later) |
| Lian Li Uni Hub AL (AL10 firmware path) | 0CF2:A101 | default (WinUSB) | same |
| Lian Li Universal Screen LED | 0416:8050 | **UsbDk** (`LIBUSB_OPTION_USE_USBDK`) | same |
| Corsair Hydro (Asetek) H100i v2 | 1B1C:0C09 | **UsbDk** | `CorsairHydro2ControllerDetect.cpp` |
| Corsair Hydro Pro H100i/H115i/H150i | 1B1C:0C15/0C13/0C12 | **UsbDk** | `CorsairHydroControllerDetect.cpp` |
| Thrustmaster Sol-R / F16 / F18 | 044F:0420..042A | **UsbDk** | `ThrustmasterSolControllerDetect.cpp` |
| Philips amBX | 0471:083F | libusb (default backend) | `AMBXControllerDetect.cpp` |

The newer Lian Li hubs (SL, AL, SL V2, AL V2, SL Infinity, Strimer L Connect, GA II) are plain HID in OpenRGB
(`REGISTER_HID_DETECTOR_IPU ... 0xFF72, 0xA1`). OpenRGB's `Documentation/USBAccess.md`: "Windows
should not need any special setup to access USB devices ... Early versions of OpenRGB used the
WinUSB driver, installed using Zadig. This is no longer required" [V]. OpenRGB solved the
problem by moving devices to HID and by using **UsbDk**, a separately installed filter driver. It
did not solve it by shipping an INF.

SignalRGB's exe embeds stock libusb with WinUSB, libusbK and UsbDk backends. Its strings include
"WinUSB DLL is not available", "UsbDk backend is available" and `GetProcAddress() failed for WinUsb_%s`
[V-local: `signalrgb_exe_strings.txt`]. So SignalRGB also relies on whatever driver the device or
the vendor software has bound. Its own kernel driver does port and PCI I/O, not USB.

### 2.2 When WinUSB binds by itself
Microsoft: "A WinUSB device is a USB device whose firmware defines certain Microsoft OS feature
descriptors that report `WINUSB` as the compatible ID ... In Windows 8, the in-box Winusb.inf file
is updated to enable Windows to automatically match the INF with a WinUSB device"
(`USB\MS_COMP_WINUSB`, class `USBDevice`) [V: learn.microsoft.com/.../usbcon/automatic-installation-of-winusb].
- MS OS 1.0: string descriptor 0xEE plus an extended compat-ID descriptor.
- MS OS 2.0: a BOS platform capability with UUID `D8DD60DF-4589-4CC7-9CD2-659D9E648A9F`, read by Windows 8.1 and later.
- The device should also publish a `DeviceInterfaceGUID(s)` extended property. Without it,
  SetupAPI clients cannot find the interface. `nusb` opens WinUSB devices through the device-instance path, so it is less affected [U].
- Windows caches the OS-descriptor probe under `HKLM\SYSTEM\CurrentControlSet\Control\usbflags\<VVVVPPPPRRRR>\osvc`.
  On this PC every RGB device reports `osvc = 00 00` (no MS OS 1.0 descriptor) [V-local]. That is
  expected, because they are all HID.

**Which rawusb families self-bind WinUSB is not established.** No such device is on this machine.
It is plausible for the hubs whose vendor apps work without a driver install (Lian Li Uni Hub
0CF2:7750, Aqua Computer) [U]. It is unlikely for the Asetek AIOs, Thrustmaster Sol and the Lian Li
Universal Screen, because OpenRGB forces UsbDk for exactly those [V for the UsbDk choice, U for the reason].

How to settle it per device, with no driver install (a task for a contributor who owns one):
1. Plug the device into a PC that has never had its vendor software or Zadig.
2. Device Manager > View > Devices by connection. If the vendor interface sits under
   "Universal Serial Bus devices" with Driver = `WinUsb`, it self-binds.
3. Or run `pnputil /enum-devices /connected /drivers` and look for `winusb.inf` on that instance.
4. Or dump the descriptors with USB Device Tree Viewer and look for an "MS OS 2.0 Platform Capability" in the BOS, or a 0xEE string.
5. Record the result in `docs/hardware/<plugin>.md`. Only devices that self-bind go into a rawusb plugin's `tested` list.

### 2.3 Options when a device does not self-bind

| Option | How | Cost | Security / UX |
|---|---|---|---|
| **A. Avoid WinUSB** | Use the HID interface where one exists (OpenRGB did this for Lian Li SL/AL). Use control transfers on an already-open HID handle (`HidD_*` only covers class requests, so vendor requests are **not** possible through `hid.dll`) [V: HID class API surface]. | none | best |
| **B. Rely on the vendor's driver** | If the vendor app (L-Connect, aquasuite, CAM) installed WinUSB/libusbK for its device, `nusb` can open it while the vendor app is closed. | none | Depends on third-party software. libusbK-bound devices are **not** reachable by `nusb`, which speaks only WinUSB [U: nusb 0.2 Windows backend is WinUSB-only]. |
| **C. libwdi-style self-signed INF** (what Zadig does) | Generate INF + CAT at runtime and sign the CAT with a freshly generated self-signed certificate. libwdi "copies two autogenerated certificates into the Root (Trusted Root Certification Authorities) and Trusted Publisher system stores"; the key is "destroyed" after signing [V: github.com/pbatard/libwdi/wiki/FAQ]. Needs elevation (`huedeck-setup`). | none | **Adds a machine-wide trusted root per install.** If the key really is destroyed, the risk is limited to the short signing window and to bugs in that code; the root is still unconstrained (any EKU) and reviewers and enterprise policy (WDAC, Intune baselines) flag it. It **replaces** the device's current function driver; for a hybrid device that would kill the HID interface. Uninstall has to remove the cert and the driver package. Not acceptable as a default; maybe acceptable behind an "advanced, I understand" switch. |
| **D. Attestation-signed WinUSB INF via Partner Center** | Package = INF referencing inbox `winusb.sys` (no binaries), CAB-signed with an **EV certificate**, submitted to Partner Center, which returns a Microsoft-signed CAT. | EV code-signing cert (typically a few hundred EUR per year, on an HSM/token, issued to a **legal entity**). The Hardware Developer Program registration itself is free [U: current pricing; EV requirement V]. | Microsoft now files attestation under "**for testing scenarios**": "Attestation signed drivers can't be published to Windows Update for retail audiences"; it "only works on Windows 10 Desktop and later"; Windows Server 2016+ refuses it [V: learn.microsoft.com/.../dashboard/driver-signing-offerings, page dated 2026-03-23]. It still installs on retail Windows 10/11 desktop when an installer stages it [V: signing-requirements table]. Shipping an INF that claims **other vendors' VID:PIDs** is a policy and support risk (conflicts with the vendor's own package and with Windows Update ranking) [U]. Each new VID:PID needs a new submission. |
| **E. WHCP/HLK-signed** | Full HLK run on each device. | Lab, hardware, legal entity | Unrealistic for a community project. |
| **F. UsbDk** (OpenRGB's route) | Users install Red Hat's signed UsbDk filter driver themselves. | none | It is a kernel filter on all USB devices. It conflicts with ADR-0007 if HueDeck ships it, so leave it out. |

### 2.4 Recommendation for raw USB
1. Implement `UsbBackend` for Windows with `nusb` (WinUSB only). Set `driver_bound = true` only when the
   interface's service is `WinUSB`. When it is not, the plugin session fails with a clear message
   ("this device needs a WinUSB driver that Windows did not install; see docs/hardware/…").
   Accept rawusb/hybrid plugins in `catalog.rs` only once that exists.
2. Do not ship or generate any INF in 1.0 (options C to F). Remove the "WinUSB binding" claim from
   ADR-0007, `docs/architecture.md` and the `huedeck-setup` description, or mark it
   "deferred, see docs/research/drivers.md".
3. Prefer a HID route in plugins whenever the device offers one (option A).
4. Revisit option D only if a legal entity with an EV certificate exists **and** a device's vendor agrees to the VID:PID claim.

## 3. AMD GPU RGB (ADL I2C)

What OpenRGB does (`i2c_smbus/Windows/i2c_smbus_amdadl.cpp`, GPL-2.0-or-later, read for facts) [V]:
- `LoadLibrary("atiadlxx.dll")` (64-bit; `atiadlxy.dll` is the 32-bit fallback). Resolves
  `ADL2_Main_Control_Create`, `ADL2_Main_Control_Destroy`, `ADL2_Adapter_NumberOfAdapters_Get`,
  `ADL2_Adapter_AdapterInfoX2_Get`, `ADL2_Adapter_AdapterInfoX4_Get` and `ADL2_Display_WriteAndReadI2C`.
- Enumerates with `AdapterInfoX4_Get(ctx, -1, …)` and keeps one adapter per PCI bus (`iBusNumber`
  de-duplication). It parses VEN/DEV/SUBSYS from `strPNPString` and keeps only vendor 0x1002.
- Transfer: `ADLI2C{iSize, iLine = 1 ("location of the Aura chip"), iAddress = addr << 1, iOffset, iAction (READ/WRITE), iSpeed = 100 kHz, iDataSize, pcData}`.
  - Reads: `iOffset = command`.
  - Writes: `iOffset = 0`, the command is **prepended** to the data, and `iDataSize = n + 1`.
- Consumers include ASRock GPU, Asus Aura GPU, ENE SMBus (GPU), Gigabyte Fusion2 GPU (+Blackwell), MSI GPU, PowerColor and Sapphire.
- OpenRGB needs no admin rights and no extra driver for this path [U: OpenRGB docs only require admin for PawnIO].

**Licence finding**: the legacy **ADL SDK** headers (`display-library/include/adl_sdk.h`,
`adl_structures.h`, `adl_defines.h`) are **MIT** ("Copyright (c) 2016 - 2022 Advanced Micro Devices,
Inc. ... MIT LICENSE") [V-local: OpenRGB `dependencies/display-library`]. That makes them
GPL-3.0-compatible. The ADLX SDK headers are not (`m5-privileged-buses.md` §5). **Use ADL, not ADLX.**

What HueDeck needs (one agent, no driver):
- A new `crates/huedeck-adl` (or a module in `huedeck-hal-windows`) that loads `atiadlxx.dll` dynamically
  from System32 only (same DLL-planting guard as `huedeck-nvapi/src/ffi.rs`), with hand-written
  `#[repr(C)]` mirrors of `ADLI2C`/`AdapterInfoX2`.
- Bus kind `AmdGpu` in the daemon. It is unprivileged, so it is **not** in the broker. Capability matrix:
  ReadByte/WriteByte/WriteBlock; WriteQuick emulated as a zero-length write [U]. Apply the
  `IsAMDGpuBus()` flag and subsystem IDs as for NvAPI.
- Mock-tested; a hardware test needs someone with an AMD card. RDNA3/RDNA4 support of
  `ADL2_Display_WriteAndReadI2C` is [U]. OpenRGB still lists RX 7000 cards (Sapphire/PowerColor), which suggests it works.

## 4. Intel chipset SMBus (`SmbusI801`)

- The PawnIO.Modules 0.2.11 blob exists and is signed by namazso. Pin: `SmbusI801.bin`, 55540 bytes,
  SHA-256 `a0f7d066e7efda28c0e754c1f52dbd8dc280d388ba8575b514a80cb67490530c`, identical to
  LibreHardwareMonitor's copy [V: `m5-privileged-buses.md` §1.5].
- Behaviour [V: same doc §1.6]: Intel 0x8086 class 0x0C05 at 00:1F.4/1F.3/1F.1/80:1F.4; MMIO
  BAR; INUSE_STS hardware semaphore; protocols 0–5, 7, 8; `ioctl_write_protection` reports SPD_WD; there is no
  `port_sel`; block-read layout differs from PIIX4 (length cell, data at byte 8).
- **Yes, pin it like PIIX4.** Add a `SMBUS_I801` `Pin` to `crates/huedeck-iobroker/src/pins.rs`, an
  `i801.rs` next to `piix4.rs` (single bus, no port select), and a detect step in `boot.rs`
  ("Intel (`SmbusI801`) can join the same way" is already noted there).
  - Keep the same mutex (`Global\Access_SMBUS.HTP.Method`), the same SPD write denial and the same refusal of block reads.
  - On DDR5 (SPD5 hub), check `ioctl_write_protection` before anything else [U: DDR5 RGB RAM behaviour on i801].
- `SmbusIntelSkylakeIMC` (Intel HEDT IMC SMBus, BYTE/WORD only) is needed only for X299-class DDR4 RGB RAM.
  Pin it later, computing its hash from the same release zip
  (`release_0_2_11.zip`, SHA-256 `43608cb8…05a81f4`).
- Never load `SmbusNCT6793`: it writes 0x87,0x87 to port 0x2E at load time without the mutex (§1.6 of the M5 brief).
- The broker can already pick whichever chipset module succeeds; the setup installer must ship
  `SmbusI801.bin` next to `SmbusPIIX4.bin`. This touches the in-flight installer/PawnIO-option branch (`feature/installer`).

## 5. SuperIO / LPC (`LpcIO`)

- Pin: `LpcIO.bin`, 18076 bytes, SHA-256 `b3896a1cab0d808fca31fe2ebcae045d59dac690da87b17c858bb8da357eb45e` [V: M5 brief].
- Its ioctls (`pio_inb/outb`, `superio_inb/inw/outb`, `select_slot`, `find_bars`) are raw port I/O, constrained only
  by the module's allow-list. The broker must expose **closed, high-level ops** (e.g. "read NCT6687D fan RPM",
  "set ARGB header colour on chip X") under `Global\Access_ISABUS.HTP.Method`, and never raw port I/O on the pipe.
- No RGB plugin in HueDeck needs it today. The MSI 7C91 board's RGB is USB HID; NCT6687D is fans and sensors only.
  Pin it only together with the first feature that uses it (fan curves, M5 stretch).

## 6. Windows Dynamic Lighting interplay

Facts from `m6-windows-integrations.md` [V there]:
- Dynamic Lighting drives **HID LampArray** collections (usage page 0x59) with inbox components; no driver is involved.
- Control priority: foreground app > ambient (background) app chosen in Settings > Windows' own effects. A device not
  selected runs "Autonomous mode".
- Background control requires **package identity** (a sparse MSIX package with the
  `com.microsoft.windows.lighting` AppExtension). A sparse package must be signed by a certificate the machine trusts:
  a real Authenticode certificate, or a self-signed one added to Trusted People, which brings the same trust-store problem as §2.3-C [V: MS sparse-package docs, via the M6 brief].

Consequences for "out of the box":
- On a device that has both a vendor HID collection (our plugin) and a LampArray collection (e.g. Razer DeathAdder V2 Pro `MI_03`),
  Windows and HueDeck can both write colours. Which one wins depends on firmware [U].
  - HueDeck should detect "Use Dynamic Lighting on my devices" as on for that device (read-only
    `HKCU\Software\Microsoft\Lighting\Devices\<id>\AmbientLightingEnabled`; the key semantics are an inference)
    and show a one-click deep link `ms-settings:personalization-lighting` telling the user to turn it off for devices HueDeck drives.
  - HueDeck must not write that registry key. SignalRGB appears to do so; it is undocumented.
- The native LampArray backend (M6) as a foreground app works unpackaged. Background (ambient) control needs the signed
  sparse package. Tie that to the Authenticode decision in §7.

## 7. Runtime DLL dependencies of the shipped binaries

Method: a 60-line read-only PE import parser (normal and delay-load tables) run with `python -I` over
`%LOCALAPPDATA%\huedeck\portable\HueDeck\*.exe|*.dll` and the M5 release builds of `huedeck-iobroker.exe`/`huedeck-setup.exe` [V-local].

| Binary | Non-system imports | Notes |
|---|---|---|
| `huedeckd.exe` | **`VCRUNTIME140.dll`** | The rest are system DLLs or the UCRT `api-ms-win-crt-*` (in Windows 10+): advapi32, bcryptprimitives, cfgmgr32, hid, iphlpapi, kernel32, ntdll, oleaut32, ws2_32. |
| `huedeck.exe` | **`VCRUNTIME140.dll`** | same set, without iphlpapi/ws2_32 |
| `huedeck-renderer.exe` | **`VCRUNTIME140.dll`**, `libcef.dll` (shipped) | |
| `huedeck-iobroker.exe`, `huedeck-setup.exe` | **`VCRUNTIME140.dll`** | The broker runs as LocalSystem from Program Files, so a missing VC++ runtime means the service fails to start. |
| `libcef.dll` | `chrome_elf.dll` (shipped) | All other imports are system DLLs, many of them delay-loaded (d3d11/12, dxgi, MF*, MMDevAPI, WINUSB, HID, …). No VC++ runtime: Chromium links its CRT statically. |
| `chrome_elf.dll`, `d3dcompiler_47.dll`, `vk_swiftshader.dll` | none | |

- `VCRUNTIME140.dll` is **not** part of Windows. It exists on this PC only because some other
  software installed the VC++ 2015–2022 redistributable [V-local: `System32\vcruntime140.dll` present].
  A clean Windows 11 install fails to start every HueDeck exe with "VCRUNTIME140.dll was not found" [U: not
  reproduced here, but standard MSVC behaviour].
- `nvapi64.dll` and `atiadlxx.dll` are loaded dynamically, so their absence is a soft failure (`huedeck-nvapi/src/ffi.rs`) [V-local].
- CEF delay-loads Media Foundation (`MF.dll`, `MFPlat.DLL`, `MFReadWrite.dll`). These are missing on Windows **N** editions
  without the Media Feature Pack. HueDeck's renderer does not use media playback, and delay-load fails only on first use [U: no N-edition test].

**Recommendation:** link the CRT statically for all Rust binaries by adding a repo `.cargo/config.toml`:

```toml
[target.x86_64-pc-windows-msvc]
rustflags = ["-C", "target-feature=+crt-static"]
```

- This removes `VCRUNTIME140.dll` and the `api-ms-win-crt-*` imports. The `cc` crate picks `/MT` automatically for bundled C code
  (SQLite, QuickJS) when `crt-static` is on [U: verify each C dependency after the change by re-running the parser].
- It is also what CEF's Windows sandbox needs (`cef_sandbox.lib` is built `/MT`) if the renderer ever enables the Chromium sandbox [U].
- Alternative: app-local `vcruntime140.dll` (allowed by the VS redistribution terms), or chaining `VC_redist.x64.exe` in the MSI.
  Both are worse: two copies to service, or an extra elevated installer.
- Verification step for CI: run the PE-import check on the staged folder and fail on any import outside an allow-list
  (System32 DLLs, `api-ms-win-*`, `libcef.dll`, `chrome_elf.dll`). This belongs to the installer agent's `tools/package/stage-windows.ps1`.
- **Done (WP-CRT):** the repo `.cargo/config.toml` sets `+crt-static` for every Windows MSVC target, and
  `tools/package/check-imports.ps1` (called by `stage-windows.ps1`) fails on any import outside the system allow-list,
  and on any C runtime DLL (`VCRUNTIME*`, `MSVCP*`, `ucrtbase`, `api-ms-win-crt-*`) [V-local: release build re-parsed].

Related, not a driver but an out-of-the-box blocker: on a clean Windows 11 install **Smart App Control**
blocks unsigned executables outright, and SmartScreen warns on download. Authenticode signing of
the exes and MSI (SignPath Foundation for OSS, or a commercial/Azure signing service) is part of
"works out of the box" [U: SAC behaviour per Microsoft docs, not tested here].

## 8. Recommendations, in order

1. **Static CRT now** (`.cargo/config.toml` + an import check in packaging). This is the one hard blocker on clean PCs; small and isolated.
2. **Pin `SmbusI801`** in the broker plus an `i801` bus; ship the blob from the installer. Mock tests; hardware test on an Intel board.
3. **AMD GPU bus over legacy ADL** (MIT headers, no driver, unprivileged), mock-tested.
4. **Raw USB via `nusb` for self-binding WinUSB devices only.** Clear message otherwise; no INF, no libwdi, no UsbDk.
   Update ADR-0007 and architecture.md to drop "huedeck-setup binds WinUSB".
5. **Dynamic Lighting**: detect it and deep-link the user to turn it off; never write its registry keys. Revisit background control with signing.
6. **Authenticode signing** of all binaries and the MSI. It also unblocks the sparse package for Dynamic Lighting background control.
7. `LpcIO` only together with the first SuperIO feature. `SmbusIntelSkylakeIMC` only on demand. Never `SmbusNCT6793`.
8. Per-device WinUSB census (§2.2 procedure) as owners report in. Record results in `docs/hardware/`.
