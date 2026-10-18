<!-- SPDX-License-Identifier: Apache-2.0 OR MIT -->
<!-- Research brief for M5. Produced 2026-10-18 by a research pass and an independent fact-check pass; facts carry inline provenance tags. -->

# M5 privileged buses: design brief for lightloom-iobroker (PawnIO, NvAPI, NCT6687D, ADLX) — independently verified

**Legend.**
- **[V: src]**: re-checked by the verifier against a primary source (URL, git tag/file, SDK header) or by a read-only check on this PC.
- **[V-local]**: read-only observation on this PC.
- **[V-sec]**: secondary source only.
- **[U]**: unverified (inference, design proposal, estimate, or untested at runtime).

**Verifier downloads (new folder).** All under `<research-cache>`:

| Subfolder | Contents |
|---|---|
| `PawnIO` | git clone; tags 1.0.0, 2.0.0, 2.1.0, 2.2.0; master 9d52965 (2026-08-14) |
| `PM` | PawnIO.Modules clone, checked out at tag 0.2.11; main = 24425a5 |
| `wiki` | PawnIO.Modules wiki clone |
| `PSetup` | PawnIO.Setup clone (releases only, no source) |
| `setup` | 2.2.0 asset listing |
| `rel0211` | `release_0_2_11.zip` and the extracted blobs |
| `winget`, `lhm`, `openrgb`, `nvapi`, `adlx` | manifests, source files and headers |
| `nct6687d` | Linux driver, GPL; facts only |
| `k10temp` | Linux k10temp.c, GPL; facts only |

Read-only parser scripts are in `...\scratchpad\verify-scripts\`.

**What was executed.** Nothing in the repo or the system was modified, and nothing was installed. The only things run on the system were:
- read-only WMI, registry, token and code-integrity queries;
- PE header and export/import parsing;
- `nvapi_QueryInterface` id lookups. No NvAPI function was called.

---
## 0. Recommendations
1. **Talk to PawnIO with raw IOCTLs from Rust.** Use the `windows` 0.62.2 crate: `CreateFileW` plus `Wdk::System::IO::NtDeviceIoControlFile`.
   - Do not load `PawnIOLib.dll` into the LocalSystem service. LibreHardwareMonitor does the same: `CreateFile` plus `DeviceIoControl`, no PawnIOLib [V: LHM PawnIo.cs].
   - `NtDeviceIoControlFile` returns the module's exact NTSTATUS. `DeviceIoControl`/`GetLastError` only gives the lossy Win32 mapping.
   - The driver's GPL exception covers "independent modules that communicate with PawnIO solely through the device IO control interface" [V: PawnIO README].
2. **Require the official PawnIO driver, 2.2.0 or later** (current release, 2026-03-15). The reasons are §1.4: BSOD fix, INF/PnP security, `IOCTL_PIO_VERSION`, PCI-native fixes, anti-cheat note.
   - **Correction:** the earlier rationale ("0.2.11 modules need natives absent in 2.0.0") is **wrong**. See §1.4.
   - Install it only on opt-in with the official `PawnIO_setup.exe -install -silent`. Pin the SHA-256 of the download, or send the user to pawnio.eu.
   - Do not embed the installer or the `.sys` in Lightloom's MSI. ADR-0007 says Lightloom never ships a `.sys` "in … the installer".
   - Never uninstall PawnIO. LHM prompts users to install it, FanControl bundles it, and OpenRGB requires it.
   - Refuse Unrestricted builds: check the code-integrity TESTSIGN flag, plus a tampered-signature probe (§1.4).
3. **Ship signed module blobs from PawnIO.Modules release 0.2.11** as separate files, each with a SHA-256 pin compiled into the broker. Ship `SmbusPIIX4`, `SmbusI801`, `LpcIO`, `Echo` (probe) and optionally `AMDFamily17` (CPU temperature).
   - **Also ship the matching `.p` sources, `sleepmode.inc` and `include/`, plus `COPYING`.** LGPL-2.1 §4 expects source to accompany object code, or "equivalent access … from the same place". A bare upstream URL is weaker [U legal reading].
   - Do **not** load `SmbusNCT6793` (§1.6).
4. **Buses on this PC** [V-local]:
   - `AMD PIIX4 Bus - 0x0b00`: port 0; the Crucial DIMMs are at 0x39/0x3A.
   - `AMD PIIX4 Bus - 0x0b20`: AUX controller.
   - `NVIDIA GeForce RTX 3060`: NvAPI, port 1; MSI controller at 0x68.
   - **No Nuvoton SMBus bus.** The NCT6687D (ID 0xD5/rev 0x92) is at LPC **0x4E**; 0x2E is empty. SmbusNCT6793 does not support this chip.
5. **Move NvAPI GPU I2C into `lightloomd` (user process); this needs an ADR-0004 amendment** [U design].
   - Evidence: a non-elevated SignalRgb.exe (TokenElevationType = Limited) opens NvAPI on driver 581.15 and runs MSI_GPU.js on the RTX 3060 at 0x68 [V-local].
   - NvAPI inside a session-0 LocalSystem service is untested [U]. Any user process can already call NvAPI, so hosting it in the service adds attack surface without adding capability.
   - Keep one `BrokerOp::validate()` policy implementation for both local and brokered buses.
6. **Mutex discipline.**
   - The broker creates or opens `Global\Access_SMBUS.HTP.Method`, `Global\Access_ISABUS.HTP.Method` and `Global\Access_PCI` with an explicit DACL. These are the literal names in the SignalRgb.exe string table, the PawnIO module docs and LHM [V].
   - Acquire them with bounded waits, on one dedicated OS thread. Win32 mutex ownership is per thread, and "ReleaseMutex fails if the calling thread does not own the mutex object" [V: learn.microsoft.com ReleaseMutex].
   - Add `BusLock`/`BusUnlock` lease ops (§6.5).
   - Fail closed (no SMBus traffic) if the mutex cannot be opened.
7. **NCT6687D fans, temperatures and voltages** (M5 stretch): use the generic LpcIO module and expose only closed, high-level ops. No raw port I/O on the wire.
8. **ADLX is low priority, and its SDK licence is not MIT.** The ADLX SDK License Agreement allows distribution only in object-code form, so the headers cannot be vendored into Apache/MIT source (§5).
9. **Fix these repo docs and code** (§8).

---
## 1. PawnIO facts
### 1.1 Licences
| Component | Licence | Source |
|---|---|---|
| Driver source | GPL-2.0-or-later plus a special exception for (a) "free software programs or libraries that are released under the GNU LGPL" and (b) "independent modules that communicate with PawnIO solely through the device IO control interface". The exception "does not include programs that communicate with PawnIO over the Pawn interface … all modules loaded into PawnIO must be compatible with this licence … We recommend … LGPL 2.1". The same text is in the `driver.cpp` header. | [V: https://github.com/namazso/PawnIO/blob/master/README.md] |
| PawnIOLib | LGPL-2.1-or-later | [V: PawnIOLib/include/PawnIOLib.h, PawnIOLib.cpp] |
| Binary editions | Wiki: "Official and unrestricted binary editions: Proprietary, however redistribution of installer is allowed"; it recommends redirecting users to pawnio.eu. pawnio.eu: "Official edition: … Identical to the open-source version, but digitally signed." and "Unrestricted edition: An unsigned version without signature checking. Can be selected in the installer." | [V: https://github.com/namazso/PawnIO.Modules/wiki/Using-PawnIO-Modules], [V: https://pawnio.eu/] |
| winget metadata | `License: Proprietary (Freeware)` | [V: https://github.com/microsoft/winget-pkgs/blob/master/manifests/n/namazso/PawnIO/2.2.0/namazso.PawnIO.locale.en-US.yaml] |
| PawnIO.Modules | `COPYING` is LGPL-2.1. Every `.p` file is LGPL-2.1-or-later (SPDX tag). `LpcIO.p` and `LpcCrOSEC.p` carry the LGPL text without an SPDX tag, and `sleepmode.inc` has no header, so the repo `COPYING` applies. | [V: tag 0.2.11] |
| `include/*.inc` | 0BSD for `core`, `extra`, `native`, `ntstatus`, `pawnio`, `util`. **Exception:** `include/registry.inc` is LGPL-2.1-or-later. | [V: tag 0.2.11] |
| New official modules | Wiki: "If you want to use the official PawnIO driver with a module, it needs to be merged to this repository … periodically built, signed, and released." "The module must be under LGPL 2.1 license." It must also be "usable by everyone (no Pawn-side lockdown)" and must not aid compromising security measures. The driver trusts only `k_pubkey_namazso_2023` (BCRYPT RSA1 blob, 4096-bit). | [V: wiki Contribution-guidelines], [V: PawnIO/src/vm.cpp @2.2.0] |

**Consequence.** Lightloom cannot load its own modules on end-user machines unless they are upstreamed under LGPL-2.1 and signed by namazso. ADR-0004's "separate GPL-2 repository" is unusable on two counts:
- The guidelines require LGPL-2.1.
- Unsigned modules load only on Unrestricted builds.

### 1.2 Device, privilege, IOCTL ABI
**Device object by version** [V: git tags]:

| Version | Device object | Device path | Security | Source |
|---|---|---|---|---|
| 2.0.0 | Legacy named device | `\Device\PawnIO`, plus `\DosDevices\PawnIO` (`k_device_win32path = \\.\PawnIO`) | `IoCreateDeviceSecure` with `SDDL_DEVOBJ_SYS_ALL_ADM_ALL = "D:P(A;;GA;;;SY)(A;;GA;;;BA)"` | `PawnIO/driver.cpp`, `PawnIO/ioctl.h` |
| 2.1.0 | Legacy named device | `\Device\PawnIO` only; **no DosDevices link** | same SDDL | `PawnIO/src/driver.cpp`, `include/pawnio_um.h` |
| 2.2.0 | Root-enumerated PnP device (`Root\PawnIO`, unnamed FDO with `FILE_DEVICE_SECURE_OPEN`) | `\Device\PawnIO` and `\DosDevices\PawnIO` (`k_device_dos_path_DEPRECATED`) are both symbolic links to the PDO name | INF `[PawnIO_Device.NT.HW]` → `HKR,,Security,,"D:P(A;;GA;;;SY)(A;;GA;;;BA)"` | `driver.cpp`, `pawnio_um.h`, `PawnIO.inf.in` |

- The 2.2.0 release note on the DosDevices link: "Restore the old unused device path for compatibility with broken third party PawnIOLib implementations" [V: https://github.com/namazso/PawnIO.Setup/releases/tag/2.2.0].
- **From Win32, use `\\?\GLOBALROOT\Device\PawnIO`.** It is the only path that works on 2.0, 2.1 and 2.2, and it is what LHM uses [V: https://github.com/LibreHardwareMonitor/LibreHardwareMonitor/blob/master/LibreHardwareMonitorLib/PawnIo/PawnIo.cs].
- PawnIOLib opens the device with `NtOpenFile(L"\\Device\\PawnIO", GENERIC_READ|GENERIC_WRITE, share R|W|D, OpenOptions=0)`. That is an asynchronous handle, so PawnIOLib waits on an event for `STATUS_PENDING` [V: PawnIOLib.cpp].

**Privilege.**
- Only SYSTEM and elevated Administrators can open the device. LocalService, NetworkService and filtered (non-elevated) admin or user tokens cannot.
- OpenRGB documents: "You must run the application as Administrator in order for PawnIO to be able to access SMBus" [V: https://gitlab.com/CalcProgrammer1/OpenRGB/-/blob/master/Documentation/SMBusAccess.md].
- A LocalSystem service is therefore the right host.
- 2.2.0 note: "It is now possible to expose the device to non-administrators, although not recommended" [V: release 2.2.0]. Lightloom must not do this.

**INF (2.2.0)** [V: PawnIO.inf.in @2.2.0]:
- `Class=SoftwareDevice`, `PnpLockdown=1`.
- Hardware ID `Root\PawnIO`. The manufacturer section targets `NTamd64/arm64.10.0...17763`, i.e. Windows 10 1809 or later.
- `DefaultDestDir = 13` (driver-store run-from).
- Service `PawnIO`: `ServiceType=1` (kernel driver), `StartType=3` (demand), `ErrorControl=1`.

**IOCTLs.** Device type 41394 (0xA1B2), `CTL_CODE(0xA1B2, fn, METHOD_BUFFERED, FILE_ANY_ACCESS)` [V: pawnio_um.h @2.1.0/@2.2.0, ioctl.h @2.0.0, LHM PawnIo.cs; values computed]:
- `IOCTL_PIO_LOAD_BINARY = 0xA1B22084` (fn 0x821). Input = signed blob. Same in every version.
- `IOCTL_PIO_EXECUTE_FN = 0xA1B22104` (fn 0x841). Input = 32-byte NUL-padded function name followed by N × u64 cells; output = M × u64 cells. Same in every version.
- `IOCTL_PIO_VERSION = 0xA1B22184` (fn 0x861). **2.1.0 and later only.** The output buffer length must be exactly 4; the value is `(maj<<16)|(min<<8)|patch`, so 2.2.0 → `0x00020200`.
  - On 2.0.x this IOCTL hits the default case and returns `STATUS_NOT_IMPLEMENTED` (0xC0000002). Treat that as "older than 2.1.0".
  - 2.0.0 instead had `IOCTL_PIO_GET_REFCOUNT` (fn 0x801 → 0xA1B22004); it is commented out from 2.1.0.
- All IOCTLs are `FILE_ANY_ACCESS`, so the access mask requested at open only matters for the open-time ACL check.

**Execution semantics** [V: driver.cpp, vm.cpp @2.2.0; diff to master]:
- **One module per handle.** A second LOAD on the same handle returns `STATUS_ALREADY_INITIALIZED` (0xC0000510).
- **Close.** `IRP_MJ_CLOSE` (last reference to the file object) calls `vm_destroy`, which runs the module's public `unload()` if it has one.
- **Load.** The module's `main()` runs during LOAD, and its return NTSTATUS becomes the LOAD result; hardware detection happens here.
- **Function name checks:**
  - the input buffer must be at least 32 bytes, otherwise `STATUS_INVALID_PARAMETER`;
  - the 32-byte name must contain a NUL, i.e. be 31 characters or fewer;
  - it must start with `ioctl_`, otherwise `STATUS_INVALID_PARAMETER`;
  - a missing public returns `STATUS_OBJECT_NAME_NOT_FOUND` (0xC0000034); no module loaded returns `STATUS_INVALID_PARAMETER`.
  - Keep names at 30 characters or fewer if PawnIOLib 2.2.0 might be used: it calls `lstrcpynA(p, name, 31)`; master copies up to 31.
- **Serialisation.** Calls are serialised per VM by a `FAST_MUTEX`. There is no lock across handles or processes.
- **Buffer aliasing.** With `METHOD_BUFFERED`, the VM maps input cells at `SystemBuffer+32` and output cells at `SystemBuffer+0`. So output cells 0–3 alias the name bytes, and output cell k aliases input cell k−4.
- **Returned size.** On success `IoStatus.Information = OutputBufferLength` (whole buffer), so the returned size does not tell you how many cells the module wrote.
- In 2.2.0, bytes beyond the input length are not zeroed; master zeroes them (unreleased).
- Mask outputs to the documented width and ignore cells that were not written.
- The module's NTSTATUS becomes the IRP status.

### 1.3 PawnIOLib exports (reference only; not recommended inside the service)
[V: https://github.com/namazso/PawnIO/blob/master/PawnIOLib/include/PawnIOLib.h; PawnIOLib.cpp @2.2.0 and master]
- Calling conventions: `PAWNIOAPI = EXTERN_C __declspec(dllexport) HRESULT STDAPICALLTYPE`, `PAWNIOWINAPI = … BOOL WINAPI`, `PAWNIONTAPI = … NTSTATUS NTAPI`. All three are `extern "system"` in Rust. Names are undecorated on x64; the wiki uses `GetProcAddress(lib, "pawnio_open")`.
- version, open, load, execute and close each come in three flavours: `name` (HRESULT), `name_win32` (BOOL with `SetLastError`), and `name_nt` (NTSTATUS).
- **Exception:** `pawnio_execute_async` exists as HRESULT and `_win32` (both take an `OVERLAPPED`), while `pawnio_execute_async_nt` has a different signature (event, APC, IOSB).
```c
HRESULT pawnio_version(PULONG version);   // LIBRARY version, hard-coded 0x00020000 — not the driver's
HRESULT pawnio_open(PHANDLE handle);      // NtOpenFile(L"\\Device\\PawnIO", GENERIC_READ|GENERIC_WRITE, share R|W|D, 0)
HRESULT pawnio_load(HANDLE h, const UCHAR* blob, SIZE_T size);
HRESULT pawnio_execute(HANDLE h, PCSTR name, const ULONG64* in, SIZE_T in_size,
                       PULONG64 out, SIZE_T out_size, PSIZE_T return_size);   // sizes in cells
HRESULT pawnio_execute_async(HANDLE, PCSTR, const ULONG64*, SIZE_T, PULONG64, SIZE_T, LPOVERLAPPED);
NTSTATUS pawnio_execute_async_nt(HANDLE, PCSTR, HANDLE event, PVOID apc, PVOID apc_ctx,
                                 PVOID iosb, const ULONG64*, SIZE_T, PULONG64, SIZE_T);
HRESULT pawnio_close(HANDLE h);
```
- The HRESULT is `HRESULT_FROM_WIN32(RtlNtStatusToDosError(st))`, which is lossy. Master maps NT success codes to `S_OK`; 2.2.0 converts every status, including success codes.
- **Wiki integration recipe:**
  1. Read `InstallLocation` from `HKLM\SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall\PawnIO`; fall back to `%ProgramFiles%\PawnIO` ("the installer does not allow changing the install path").
  2. Call `LoadLibrary(InstallLocation + "PawnIOLib")`.
  - [V: wiki Using-PawnIO-Modules]

### 1.4 Blob format, signing, editions, version floor
**Blob format.**
- Layout: `u32 LE sig_len | sig[sig_len] | AMX image`.
- The driver checks the SHA-256 of the AMX image with `BCryptVerifySignature(…, BCRYPT_PAD_PKCS1, SHA256)` against the trusted key list.
- All 23 blobs in the 0.2.11 zip have `sig_len = 512` and AMX magic `0xF1E1` (64-bit cells) at AMX offset 4.
- [V: vm.cpp/signature.cpp @2.2.0; V-local: parsed every blob]

**Unrestricted build** [V: vm.cpp, PawnIO/CMakeLists.txt]:
- `#ifdef PAWNIO_UNRESTRICTED` prints "Signature check result" and forces `STATUS_SUCCESS`.
- The wiki says "The unrestricted driver is only test signed" and needs TESTSIGNING [V: wiki Getting-started-with-PawnIO]. pawnio.eu calls it "unsigned" and says it "Can be selected in the installer" [V].

**Detecting the edition.**
1. **Code-integrity flags** [V: SDK `um/winternl.h`]: call `NtQuerySystemInformation(SystemCodeIntegrityInformation = 103)` and read `SYSTEM_CODEINTEGRITY_INFORMATION.CodeIntegrityOptions`. If `CODEINTEGRITY_OPTION_TESTSIGN` (0x02) is clear, a test-signed Unrestricted driver cannot be loaded.
   - This PC reports `0x0000F401` [V-local]: TESTSIGN off; `HVCI_KMCI_ENABLED` 0x400, `HVCI_KMCI_STRICTMODE_ENABLED` 0x1000 and `HVCI_IUM_ENABLED` 0x2000 on.
2. **Tamper probe** [U, designed from verified code]:
   1. On a fresh handle, load `Echo.bin` with one signature byte flipped. On official builds this must fail (expected `STATUS_INVALID_SIGNATURE` 0xC000A000 from BCrypt). If it loads, refuse to run.
   2. Echo's `main()` only calls `get_arch` and `debug_print`, so the probe is harmless.
   3. Then load the genuine `Echo.bin` and check `ioctl_not(x) == !x` (bitwise NOT).

**Version floor (corrected).**
- **Wrong earlier claim:** "get_proc_address/invoke absent in 2.0.0". The 2.0.0 native table includes `get_proc_address` and `invoke` [V: `PawnIO/vm.cpp` @2.0.0].
- Parsing the AMX native import tables of the 0.2.11 blobs (`SmbusPIIX4`, `SmbusI801`, `SmbusNCT6793`, `LpcIO`, `AMDFamily17`, `Echo`) shows every imported native exists in the 2.0.0, 2.1.0 and 2.2.0 tables [V-local]. Runtime compatibility with 2.0.x/2.1.x is untested [U].
- The modules' `microsleep2` is a Pawn stock built on `get_proc_address("KeStallExecutionProcessor")` plus `invoke`, not the driver native.
- **Still require 2.2.0 or later**, because:
  - "Fix BSODs on Windows 10 RS5, 19H1, 19H2 due to missing exports" [V: release notes];
  - PnP/INF device security;
  - `IOCTL_PIO_VERSION` exists only from 2.1.0;
  - 2.1.0 carries "Fix broken error handling in PCI functions" (commit 50a8c14) and the cmpxchg-native fixes;
  - FanControl V259 notes "Support for PawnIO 2.1.0 driver (should fix FaceIT issue)" [V: https://github.com/Rem0o/FanControl.Releases/releases/tag/V259].

### 1.5 Modules available (release 0.2.11)
Release https://github.com/namazso/PawnIO.Modules/releases/tag/0.2.11, published 2026-08-30T01:58Z [V].
- `release_0_2_11.zip`: SHA-256 `43608cb89bc84247fef1368a139013f7d043e17db6d6c8dfc9b46bf0905a81f4`, matching GitHub's asset digest [V].
- Blobs: AMDFamily0F, AMDFamily10, AMDFamily17, AMDReset, ARMMSR, DellSMM, Echo, IntelMCHBAR, IntelMSR, IntelOOBMSM, IntelPCHThermal, IsaBridgeEC, LedsValve, LpcACPIEC, LpcCrOSEC, LpcIO, Nvidia, RyzenSMU, SmbusI801, SmbusIntelSkylakeIMC, SmbusNCT6793, SmbusPIIX4, ZhaoxinMSR, plus `COPYING` [V-local].
  - `Nvidia.bin` exposes only `ioctl_read_thermal_registers` and `ioctl_read_memory_temperatures`.
  - Main adds `PcSpeaker.p`, which is unreleased. Between 0.2.11 and main only `IntelMSR.p` and `PcSpeaker.p` differ, so every SMBus and LPC source is identical [V].

**Pins** (SHA-256, computed locally). LHM's bundled copies of PIIX4, LpcIO, I801 and AMDFamily17 have identical hashes [V].

| Module | Size (bytes) | SHA-256 |
|---|---|---|
| SmbusPIIX4.bin | 42404 | `91f9b4b1c39e3d399ce48477a89d8f6bd3e58a2241064a4daec3a1513dff56e5` |
| SmbusI801.bin | 55540 | `a0f7d066e7efda28c0e754c1f52dbd8dc280d388ba8575b514a80cb67490530c` |
| LpcIO.bin | 18076 | `b3896a1cab0d808fca31fe2ebcae045d59dac690da87b17c858bb8da357eb45e` |
| SmbusNCT6793.bin | 21244 | `db068c6a87c0066ebfb7f691d88422fee8af350898e7a7f0db100fd6632bab17` |
| AMDFamily17.bin | 10652 | `dae74615761b78bdf064dfb3e136252ddcc6fc727d88f14738d0e5800d427a91` |
| IntelMSR.bin | 5324 | `d6ed85d65ab17a22f813ef98207d6d537155ee2ded5976a21cb48413c9b92e5f` |
| Echo.bin | 2284 | `96cb194cbbf1bd3ced0c0d5e2ac8ec55a5f8964b305dac005a30ac9b72787696` |

**Public functions**, read from the AMX public tables [V-local] and matching the sources:
- **PIIX4:** `ioctl_clock_freq`, `ioctl_identity`, `ioctl_piix4_port_sel`, `ioctl_set_sleep_mode`, `ioctl_smbus_xfer`.
- **I801:** the same minus `port_sel`, plus `ioctl_write_protection` and `unload`.
- **LpcIO:** `ioctl_select_slot`, `ioctl_find_bars`, `ioctl_pio_inb`, `ioctl_pio_outb`, `ioctl_superio_inb`, `ioctl_superio_inw`, `ioctl_superio_outb`.
- **Echo:** `ioctl_not` and `unload`.
- **AMDFamily17:** `ioctl_read_msr`, `ioctl_write_msr`, `ioctl_read_smn`.
  - MSR access is allow-listed. Writes are allowed to P-state 0–7, HWCR, EVNTSEL0/PERFCTR0, PMGT_MISC, CSTATE_CONFIG, CPPC_ENABLE/REQ/STATUS and LS/IC/DC_CFG/LS_CFG2.
  - `main()` accepts AMD families 0x17–0x1A.
  - The broker must never forward raw MSR or SMN on the wire.

### 1.6 SMBus module ioctls (0.2.11 = main)
**Ioctl macros** [V: `include/pawnio.inc`]:
- `DEFINE_IOCTL_SIZED(name, in, out)` returns `STATUS_INVALID_PARAMETER` unless `in_size == in` (when `in` ≠ 0) and `out_size == out` (when `out` ≠ 0).
- `DEFINE_IOCTL` is unsized.

Protocol codes are the Linux `I2C_SMBUS_*` sizes: QUICK=0, BYTE=1, BYTE_DATA=2, WORD_DATA=3, PROC_CALL=4, BLOCK_DATA=5, BLOCK_PROC_CALL=7, I2C_BLOCK_DATA=8. Direction: Read=1, Write=0.

**`ioctl_smbus_xfer`** (unsized; `in_size` ≥ 4) [V: https://github.com/namazso/PawnIO.Modules/blob/0.2.11/SmbusPIIX4.p]:
- **Inputs:** `in[0]` = 7-bit address (0..0x7F, else `STATUS_INVALID_PARAMETER`), `in[1]` = direction (0 or 1, else `STATUS_INVALID_PARAMETER`), `in[2]` = command, `in[3]` = protocol, `in[4]` = data.
- Writes of BYTE, BYTE_DATA and WORD_DATA need `in_size` ≥ 5.
- For BYTE **write** (send byte), the byte on the wire is `in[2]`.
- **Block write:** needs `in_size` ≥ 9. `in[4..8]` hold 40 bytes, little-endian byte-packed: byte 0 = length (1..32), then the data.
- **Outputs:** a byte or word read lands in `out[0]` (full cell; word = DAT0 | DAT1<<8). A block read is byte-packed `[len, data…]` in `out[0..4]` and needs `out_size` ≥ 5.
- **Supported protocols:** QUICK, BYTE (receive and send), BYTE_DATA, WORD_DATA, BLOCK_DATA. There is no PROC_CALL and no I2C_BLOCK_DATA.
- **Behaviour:**
  - If the PCI command I/O bit on 00:14.0 is clear, it sets the bit and afterwards clears only that bit again.
  - Timeout is 64 ms; on timeout it writes KILL and returns `STATUS_IO_TIMEOUT` 0xC00000B5.
  - Status bit 0x04 (device error/NACK) → `STATUS_NO_SUCH_DEVICE` 0xC000000E. This check runs last, so it wins over the others.
  - Status bit 0x10 (failed) or 0x08 (bus collision; log "Bus collision! SMBus may be locked until next hard reset. (sorry!)") → `STATUS_IO_DEVICE_ERROR` 0xC0000185.
  - Busy and not clearable → `STATUS_DEVICE_BUSY` 0x80000011 (warning class; `NT_SUCCESS` is false).
  - Bad block length from the device → `STATUS_DEVICE_PROTOCOL_ERROR` 0xC0000186.
  - All values confirmed in the Windows SDK 10.0.26100 `ntstatus.h`.
- **Load-time `main()`:**
  - Wrong architecture, PCI 00:14.0 not 1022:790B, revision below 0x51, FCH PM MMIO 0xFED80300 not mappable, SMBus-enable bit (0x10) clear, or status register reading 0xFF → **`STATUS_NOT_SUPPORTED` (0xC00000BB)**.
  - Only an SMBus base other than 0xB00 → `STATUS_NOT_IMPLEMENTED` (0xC0000002).
  - **Correction:** the earlier brief implied `STATUS_NOT_IMPLEMENTED` for all of these.
  - On this PC the device is `PCI\VEN_1022&DEV_790B&SUBSYS_790B1022&REV_61` at location A0 = 00:14.0, so the PCI checks pass [V-local]. The MMIO checks are unverified at runtime [U].

**Other PIIX4 ioctls** [V]:

| Ioctl | In → out cells | Meaning |
|---|---|---|
| `ioctl_identity` | (0, 3) | `out[0]` = `CHAR5_CONST('P','I','I','X','4')`, little-endian packed. `out[1]` = the handle's current I/O base. `out[2]` = dword@0x00 \| dword@0x2C << 32 = `vid \| did<<16 \| subvid<<32 \| subdid<<48`. Expected here: `0x790B1022790B1022` [U runtime]. |
| `ioctl_piix4_port_sel` | (1, 1) | `in[0]` ∈ {−1, 0..4}; `out[0]` = previous port. See §1.7. |
| `ioctl_clock_freq` | (1, 1) | Hz, or −1 to query. The argument check is 10 kHz–1 MHz, but `SMBTIMING = 66 MHz/freq/4` must be 1..255, so the effective minimum is about 64.7 kHz. Default 93.75 kHz. |
| `ioctl_set_sleep_mode` | (1, 0) | 0 = AlwaysBusy (module default; `KeStallExecutionProcessor`). 1 = ShortBusy (stall for polls, `KeDelayExecutionThread` for the long wait). 2 = AlwaysSleep. Verified in `natives_impl_windows.cpp` (`microsleep` = `KeDelayExecutionThread`) and in `include/extra.inc`. |

- Every transfer doc carries "@warning You should acquire the "\BaseNamedObjects\Access_SMBUS.HTP.Method" mutant before calling this".
- No module acquires a mutex itself: no wait or mutant APIs are used [V].
- `SmbusIntelSkylakeIMC.p` states: "The global SMBus mutex has to be used to stay compliant with other software. For example we have received confirmation of HWiNFO and SIV to use the SMBus mutex" [V].

**SmbusI801** [V: SmbusI801.p]:
- Device: Intel 0x8086, class 0x0C05, at 00:1F.4, 00:1F.3, 00:1F.1 or 80:1F.4.
- Uses the MMIO BAR (SMBMBAR) since tag 0.2.10 (commit 3a4a5c9).
- Implements the INUSE_STS hardware semaphore.
- Supports protocols 0–5, 7 and 8. For an I2C_BLOCK_DATA read, `in[4]` is the requested length.
- **Block reads** (BLOCK_DATA, I2C_BLOCK_DATA, BLOCK_PROC_CALL): `out[0]` is the length as a full cell and the data starts at byte 8. **This differs from PIIX4.**
- `ioctl_identity` returns "i801", the I/O base (if one is assigned) and the PCI IDs. `ioctl_clock_freq` reports a fixed 100 000 Hz; setting a value returns `STATUS_NOT_SUPPORTED`. `ioctl_write_protection` returns the SPD_WD bit. Timeout is 80 ms. It has an `unload()`.

**SmbusNCT6793** [V]:
- `DEFINE_IOCTL_SIZED(ioctl_smbus_xfer, 9, 5)`. Protocols: BYTE_DATA, WORD_DATA, BLOCK_DATA. Writes results with `SET_BYTE_LE`.
- Probes **only 0x2E**. Supported chips by `ID & 0xFFF8`: 0xC800 NCT6791, 0xC910 NCT6792, 0xD120 NCT6793, 0xD350 NCT6795, 0xD420 NCT6796, 0xD428 NCT6798. SMBus base comes from LDN 0x0B, register 0x62.
- **Do not load it.** Its `main()` writes 0x87, 0x87 to port 0x2E at load time without any mutex and never writes 0xAA (no config-mode exit). It also does not support this PC's NCT6687D: 0xD592 & 0xFFF8 = 0xD590, which is not in the list.

**SmbusIntelSkylakeIMC** [V]: `ioctl_smbus_xfer` (9, 5) supporting BYTE_DATA and WORD_DATA (QUICK, BLOCK_DATA and I2C_BLOCK_DATA are rejected); `ioctl_smbus_index` (1, 0) with `in[0] & 1`; `ioctl_identity` returns "IntelIMC".

**Client rule:** always call `ioctl_smbus_xfer` with `in_size = 9` and `out_size = 5`, zero-filled, as OpenRGB does. Mask results to 8 or 16 bits.

### 1.7 PIIX4 port selection (0xB00 vs 0xB20)
[V: SmbusPIIX4.p]
- **Initial state:** every handle starts with `piix4_smba = 0x0B00`.
- **Port 1 = AUX controller at 0xB20.** Selecting it reads the mux but does not write it, then switches the handle's `piix4_smba`.
  - It first runs `piix4_busy_check()` on the handle's current base, which may write the status register, and maps FCH MMIO. **Hold the SMBus mutex for this call too.**
- **Ports 0, 2, 3, 4 = the primary controller at 0xB00 behind a hardware mux.** The mux is FCH PM MMIO `0xFED80300 + 0x02`, bits [4:3]; port→register mapping {0→0b00, 2→0b01, 3→0b10, 4→0b11}; reverse mapping `reg_to_port = [0, 2, 3, 4]`.
  - The mux is global hardware state. Module doc: "Changing to port 2, 3, or 4 may break other software expecting to be using port 0"; "Ports 3 and 4 are marked as reserved in the datasheet".
- **Recipe (same as OpenRGB):**
  - Open two handles, each loading SmbusPIIX4.bin.
  - Under the SMBus mutex, call `port_sel(0)` on handle A ("AMD PIIX4 Bus - 0x0b00") and `port_sel(1)` on handle B ("AMD PIIX4 Bus - 0x0b20").
  - Never select ports 2–4.
  - [U] For robustness, at the start of each locked batch on handle A, call `port_sel(0)` (`out[0]` = previous port) and restore the previous port afterwards if it differed.
- **On this PC:** SignalRGB logs "PIIX4 Bus at 0x0b00 host status register: 0x00" and the same for 0x0b20. The Crucial DIMMs are found on 0x0b00 at addresses 57 and 58 (0x39/0x3A) [V-local: `%USERPROFILE%\AppData\Local\WhirlwindFX\SignalRgb\Logs\SignalRGB_20261018_102041.log` lines 167–170, 521–529].

### 1.8 How OpenRGB maps SMBus onto PawnIO (GPL-2.0-or-later; facts only, no code copied)
[V: https://gitlab.com/CalcProgrammer1/OpenRGB/-/blob/master/i2c_smbus/Windows/i2c_smbus_pawnio.cpp, `.h`]
- **Packaging.** OpenRGB ships its own `dependencies/PawnIO/PawnIOLib.dll`, `.lib` and `.h`, plus `modules/` with LedsValve, LpcIO, SmbusI801, SmbusIntelSkylakeIMC, SmbusNCT6793 and SmbusPIIX4 `.bin` [V: GitLab tree API].
- **Detection.** It calls `pawnio_version()`, which only proves the bundled DLL loaded (the library version is hard-coded). The driver's presence is only discovered at open/load.
- **Module loading.** Modules are read from the executable's directory, one PawnIO handle per bus:
  - `SmbusI801.bin` ("i801");
  - `SmbusPIIX4.bin` twice: `port_sel(0)` → "piix4_0", `port_sel(1)` → "piix4_1";
  - `SmbusNCT6793.bin`;
  - `SmbusIntelSkylakeIMC.bin` twice, with `ioctl_smbus_index` 0 and 1.
  - A load result of `HRESULT_FROM_WIN32(ERROR_NOT_SUPPORTED)` is logged "PawnIO module initialization aborted (unsupported)".
- **Per bus:**
  - `ioctl_identity` (in = 1, out = 3) decodes the 8-character name and the PCI IDs.
  - `ioctl_set_sleep_mode` with setting `smbus_sleep_mode`, default `PAWNIO_SLEEPMODE_ALWAYSSLEEP` (2).
  - If `shared_smbus_access` (default true): `CreateMutexA(NULL, FALSE, "Global\\Access_SMBUS.HTP.Method")`, then `WaitForSingleObject(INFINITE)` around every `ioctl_smbus_xfer` and around `port_sel`.
  - If `CreateMutexA` returns NULL, it runs **unlocked**.
- **Transfer:**
  - `in = {addr, rw, command, size}` followed by a `memcpy` of the 34-byte `i2c_smbus_data` union into `in[4..]`; `in_size = 9`, `out_size = 5`; the output is copied back into the union.
  - Any non-zero HRESULT becomes −1.
  - `i2c_xfer` (raw) returns −1.
- **Shared mutex history.** The mutex was added by MR 1436 "Shared SMBus Access Using Global Mutex on Windows" (merged 2022-10-14), "to allow sharing access to the smbus with other applications like HWINFO, SIV" [V: GitLab API https://gitlab.com/CalcProgrammer1/OpenRGB/-/merge_requests/1436].

### 1.9 How LHM uses PawnIO (MPL-2.0; facts only)
[V: https://github.com/LibreHardwareMonitor/LibreHardwareMonitor/tree/master/LibreHardwareMonitorLib/PawnIo]
- **Device access:**
  - `CreateFile(@"\\?\GLOBALROOT\Device\PawnIO", (uint)FileAccess.ReadWrite /* = FILE_READ_DATA|FILE_WRITE_DATA */, share R|W, OPEN_EXISTING)`;
  - `DeviceIoControl` with `(41394<<16)|(0x821<<2)` and `(41394<<16)|(0x841<<2)`;
  - no PawnIOLib.
- **Installation detection:** reads `DisplayVersion` under `Uninstall\PawnIO`.
- **Blobs:** the 0.2.11 blobs are embedded as resources (`Resources/PawnIo/README`: "Signed PawnIO modules from the official repository, belonging to the 0.2.11 release").
- **Installer:**
  - `LibreHardwareMonitor.Windows.Forms/Resources/PawnIO_setup.exe` exists in the repo.
  - `MainForm.cs` offers to install PawnIO if it is missing or older than **2.0.0**, extracts the installer and runs `PawnIO_setup.exe -install` (not silent) — a redistribution precedent.
- **Mutexes** [V: Hardware/Mutexes.cs]:
  - Creates `Global\Access_ISABUS.HTP.Method`, `Global\Access_PCI`, `Global\Access_EC`, `Global\RazerReadWriteGuardMutex` and `Global\Access_USB_Sensors` with a WorldSid FullControl ACL, falling back to `OpenExisting` on `UnauthorizedAccessException`.
  - Treats `AbandonedMutexException` as acquired.
  - It does not create the SMBus mutex in this file.

### 1.10 Installing PawnIO on end-user machines
**Installer.**
- URL `https://github.com/namazso/PawnIO.Setup/releases/download/2.2.0/PawnIO_setup.exe`, released 2026-03-15.
- SHA-256 `1F519A22E47187F70A1379A48CA604981C4FCF694F4E65B734AAA74A9FBA3032`, matching GitHub's asset digest.
- winget lists the same file for x64 and arm64.
- [V: release expanded_assets; winget installer.yaml]

**winget** [V: https://github.com/microsoft/winget-pkgs/blob/master/manifests/n/namazso/PawnIO/2.2.0/namazso.PawnIO.installer.yaml]:
- Id `namazso.PawnIO`; versions 2.0.1, 2.1.0, 2.2.0 [V].
- Manifest fields:
  - `InstallerType: exe`, `Scope: machine`, `InstallModes: silent, silentWithProgress`;
  - silent switches `-install -silent`;
  - `InstallerSuccessCodes: [3010]` (0 is implicitly success);
  - `ElevationRequirement: elevatesSelf`, `UpgradeBehavior: uninstallPrevious`, `RepairBehavior: uninstaller`, `ProductCode: PawnIO`.

**Release notes:**
- 2.2.0: "Returned codes for CLI mode are now DOS errors instead of NTSTATUS"; "In silent mode ERROR_SUCCESS_REBOOT_REQUIRED is appropriately returned if a restart is needed" (3010).
- 2.0.1: "If named "uninstall.exe", setup will behave as uninstaller" [V: release pages].
- The PawnIO.Setup repo hosts binaries only ("This repository hosts PawnIO official releases."), so the other CLI switches and the default edition in `-silent` mode cannot be verified from source [U].

**Detection:** `HKLM\SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall\PawnIO`, values `InstallLocation` (wiki) and `DisplayVersion` (LHM), then open the device and query `IOCTL_PIO_VERSION`.

**This PC today** [V-local]:
- No `Uninstall\PawnIO` key and no `PawnIO` service.
- `SignalRgbDriver` (`C:\WINDOWS\system32\Drivers\SignalRgbDriver.sys`) is running, Auto start.
- VBS is running; HVCI is running (Win32_DeviceGuard SecurityServicesRunning = 2; HVCI Enabled = 1; CI options 0xF401 including strict mode).
- `VulnerableDriverBlocklistEnable = 1`.
- OS: Windows 11 Pro 26200.

**Build flags.** The driver target in `PawnIO/CMakeLists.txt` @2.2.0 uses `/guard:ehcont`, `/INTEGRITYCHECK` (compile and link), `/CETCOMPAT` (x64), NX cache-aligned pools in `vm.cpp`, and `inf2cat /pageHashes`.
- The top-level `/INTEGRITYCHECK` was removed in commit e1043d7 (user-mode targets).
- Together this suggests HVCI readiness, but **PawnIO under HVCI strict mode on this PC is untested** [U].
- Whether the official `.sys` is Microsoft attestation-signed was not checked; the installer binary was not downloaded [U].

### 1.11 Coexistence (WinRing0, SignalRGB, HWiNFO, OpenRGB, LHM, FanControl)
**Handles and mutexes.** Every client owns its own PawnIO handle and VM, and the kernel serialises only within a handle [V]. Cross-process safety therefore depends on the named mutexes:
- `Access_SMBUS.HTP.Method`: SMBus xfer, port_sel, clock.
- `Access_ISABUS.HTP.Method`: every LpcIO call.
- `Access_PCI`: AMDFamily17 `ioctl_read_smn` and RyzenSMU.
- Source: the module `@warning` docs [V].

**Other kernel drivers.** WinRing0, inpoutx64 and SignalRgbDriver.sys can be loaded alongside PawnIO; there is no kernel arbitration, so a tool that ignores the mutexes can corrupt transactions [U reasoning].
- SignalRgbDriver.sys imports only `HalGetBusDataByOffset`/`HalSetBusDataByOffset` from HAL (PCI config). There is no `MmMapIoSpace` and nothing I2C-related; port I/O needs no import [V-local import scan].

**SignalRGB on this PC** [V-local]:
- It runs non-elevated: PID 19420, `TokenElevation = 0`, `TokenElevationType = 3` (Limited). The logs also say "SignalRGB is not running as Administrator!".
- At startup it **creates** the SMBus, ISA, PCI, CorsairLink and UsbSensor mutexes (log lines 143–157). So a non-elevated process can create these `Global\` mutexes.
- SignalRgb.exe contains the literal strings `Global\Access_SMBUS.HTP.Method` and "Access was denied when trying to open the %1 mutex. This is typically caused by another application creating it first and setting it to require admin rights to use. Please restart SignalRGB as Administrator…" [V-local string scan].
- The log shows "Motherboard fan control enabled: true", so SignalRGB actively drives the NCT6687D fans.

**WinRing0.**
- Defender name `HackTool:Win32/Winring0` [V: https://github.com/Rem0o/FanControl.Releases/discussions/3017].
- CVE-2020-14979: "The WinRing0.sys and WinRing0x64.sys drivers 1.2.0 in EVGA Precision X1 through 1.0.6 allow local users, including low integrity processes, to read and write to arbitrary memory locations…" [V: MITRE CVE API].
- FanControl replaced WinRing0 with PawnIO (V239, per that discussion), and V265 "Update included version of PawnIO to 2.2.0", i.e. it bundles PawnIO [V: release pages].

**Mutex DACL hazard.**
- [V: CreateMutexW docs] "If lpMutexAttributes is NULL, the mutex gets a default security descriptor. The ACLs in the default security descriptor for a mutex come from the primary or impersonation token of the creator."
- [V: CreateMutexW docs] "If lpName matches the name of an existing named mutex object, this function requests the MUTEX_ALL_ACCESS access right."
- Therefore, if the broker (SYSTEM) creates a mutex with NULL attributes, other clients may be denied:
  - SignalRGB then stops I2C;
  - OpenRGB gets NULL and runs unlocked.
  - The exact ACEs of the SYSTEM default DACL are [U].
- **Mitigation, as LHM does:** create with an explicit SDDL `D:(A;;0x001F0001;;;WD)`. `MUTEX_ALL_ACCESS` = 0x1F0001 [V: Synchronization Object Security docs].
- This also lets any process `WRITE_DAC` or hold the mutex (DoS). Use bounded waits, report contention, and fail closed [U].

---
## 2. Rust: raw IOCTLs (recommended) vs PawnIOLib.dll
**Recommendation: raw IOCTLs** via `windows` 0.62.2 (MIT OR Apache-2.0; crates.io max_stable 0.62.2) [V].

**Verified signatures** (windows 0.62.2 docs) [V: microsoft.github.io/windows-docs-rs pages]:
- `CreateFileW<P0: Param<PCWSTR>>(lpfilename: P0, dwdesiredaccess: u32, dwsharemode: FILE_SHARE_MODE, lpsecurityattributes: Option<*const SECURITY_ATTRIBUTES>, dwcreationdisposition: FILE_CREATION_DISPOSITION, dwflagsandattributes: FILE_FLAGS_AND_ATTRIBUTES, htemplatefile: Option<HANDLE>) -> Result<HANDLE>`
- `DeviceIoControl(hdevice: HANDLE, dwiocontrolcode: u32, lpinbuffer: Option<*const c_void>, ninbuffersize: u32, lpoutbuffer: Option<*mut c_void>, noutbuffersize: u32, lpbytesreturned: Option<*mut u32>, lpoverlapped: Option<*mut OVERLAPPED>) -> Result<()>`
- `windows::Wdk::System::IO::NtDeviceIoControlFile(filehandle: HANDLE, event: Option<HANDLE>, apcroutine: PIO_APC_ROUTINE, apccontext: Option<*const c_void>, iostatusblock: *mut IO_STATUS_BLOCK, iocontrolcode: u32, inputbuffer: Option<*const c_void>, inputbufferlength: u32, outputbuffer: Option<*mut c_void>, outputbufferlength: u32) -> NTSTATUS`
- `CreateMutexW<P2: Param<PCWSTR>>(lpmutexattributes: Option<*const SECURITY_ATTRIBUTES>, binitialowner: bool, lpname: P2) -> Result<HANDLE>`

**Cargo features** (verified to exist in windows 0.62.2's feature table on crates.io) [V]: `Win32_Foundation`, `Win32_Storage_FileSystem`, `Win32_System_IO`, `Win32_System_Threading`, `Win32_Security`, `Win32_Security_Authorization`, `Wdk_System_IO`, `Win32_System_LibraryLoader`, `Win32_System_Services`, `Win32_System_Pipes`, `Win32_System_Registry`.

**No existing crate.** There is no `pawnio` crate on crates.io ("crate `pawnio` does not exist") [V].

```rust
// lightloom-iobroker/src/pawnio.rs — signatures only
pub const PAWNIO_PATH: &str = r"\\?\GLOBALROOT\Device\PawnIO";
pub const IOCTL_PIO_LOAD_BINARY: u32 = 0xA1B2_2084;
pub const IOCTL_PIO_EXECUTE_FN:  u32 = 0xA1B2_2104;
pub const IOCTL_PIO_VERSION:     u32 = 0xA1B2_2184; // 2.1.0+; out exactly 4 bytes; 2.0.x -> STATUS_NOT_IMPLEMENTED
pub const FN_NAME_LEN: usize = 32;                  // NUL-padded; "ioctl_" prefix; <=31 chars (<=30 for PawnIOLib 2.2.0 parity)
#[derive(Clone, Copy, Debug, PartialEq, Eq)] pub struct NtStatus(pub i32);
pub struct PawnIoDevice(std::os::windows::io::OwnedHandle);          // opened, no module yet
pub struct PawnIoModule { h: std::os::windows::io::OwnedHandle, name: &'static str, sha256: [u8; 32] }
impl PawnIoDevice {
    pub fn open() -> Result<Self, NtStatus>;                          // CreateFileW(GENERIC_READ|GENERIC_WRITE, FILE_SHARE_READ|WRITE, OPEN_EXISTING), synchronous handle
    pub fn driver_version(&self) -> Result<u32, NtStatus>;            // 0x00020200 for 2.2.0
    pub fn load(self, blob: &[u8], pinned: &[u8; 32]) -> Result<PawnIoModule, NtStatus>; // verify SHA-256 of whole file first
}
impl PawnIoModule {
    pub fn execute(&self, func: &core::ffi::CStr, input: &[u64], output: &mut [u64]) -> Result<(), NtStatus>; // one buffer: 32-byte name + cells; NtDeviceIoControlFile(event=None)
}
```

**If PawnIOLib is ever needed** (for example a diagnostics CLI), use `libloading` 0.9.0 (ISC):
- `os::windows::Library::load_with_flags(filename: impl AsFilename, flags: u32) -> Result<Library, Error>`
- `get<T>(&self, symbol: impl AsSymbolName) -> Result<Symbol<T>, Error>`
- [V: https://docs.rs/libloading/0.9.0/libloading/os/windows/struct.Library.html]

All exports are `extern "system"` (stdcall = x64 C ABI):
```rust
type PawnioOpenNt    = unsafe extern "system" fn(handle: *mut HANDLE) -> NTSTATUS;
type PawnioLoadNt    = unsafe extern "system" fn(handle: HANDLE, blob: *const u8, size: usize) -> NTSTATUS;
type PawnioExecuteNt = unsafe extern "system" fn(handle: HANDLE, name: *const c_char, input: *const u64, in_cells: usize,
                                                output: *mut u64, out_cells: usize, returned_cells: *mut usize) -> NTSTATUS;
type PawnioCloseNt   = unsafe extern "system" fn(handle: HANDLE) -> NTSTATUS;
type PawnioVersionNt = unsafe extern "system" fn(lib_version: *mut u32) -> NTSTATUS; // library version only
```

**Safety constraints:**
1. One module per handle; never reload on a handle. Close on drop, which runs the module's `unload`.
2. Zero-initialise input and output. Mask outputs. Never read output cells the module did not write: they alias the name or input bytes, or contain stale pool data on 2.2.0. Ignore `Information`.
3. Call `ioctl_smbus_xfer` with in = 9 and out = 5. Block payloads are little-endian packed from `in[4]` with byte 0 = length (1..32). PIIX4 and I801 block-read layouts differ, so do not expose SMBus block reads on chipset buses.
4. Make all PawnIO calls and all named-mutex waits on one dedicated `std::thread`, never on a tokio task. Mutex ownership is per thread [V: ReleaseMutex docs].
5. Check the pinned hash before every load. Reject a driver older than 2.2.0, or an Unrestricted one (TESTSIGN flag plus tamper probe). Store blobs in a directory writable only by SYSTEM and Administrators.
6. Validate every argument before the IOCTL:
   - address 0x03–0x77;
   - direction 0 or 1;
   - length 1..32;
   - PIIX4 port ∈ {0, 1} only;
   - LpcIO slot 1 only on this PC.
7. In-process FFI cannot catch SEH. PawnIO faults are kernel-side. SignalRgb.exe contains the handler string "NVAPI access violation while polling GPU '{}'. Disabling GPU monitoring for this session (likely a sleep/resume driver remap)." [V-local], so keep NvAPI on its own thread and consider a helper process [U].

---
## 3. Nuvoton NCT6687D (fans, PWM, temperatures)
**Location on this PC** [V-local]:
- In all **5** local SignalRGB logs that contain an LPC scan (`…\Logs\SignalRGB_20261018_102041.log` lines 737–754, plus `.old.1`, `.old.4`, `.old.8`, `.old.9`):
  - port 0x2E reads chip ID 0xFF/0xFFFF ("No chip found");
  - port **0x4E** reads chip ID `0xD5`, revision `0x92` → "Found Chip: Nuvoton NCT6687D Revision: 146", "Address: 0xa20, Address Verify: 0xa20", "Fintek Vendor ID: 0xdf39".
- `docs/host-api/smbus.md` (critic gap, line 226) wrongly says the chip is at 0x2E.

**Chip variant.**
- LHM maps ID 0xD5, rev 0x92 to `NCT6687DR` only for MSI boards whose product name contains B840/B850/B860/X870/Z890; otherwise it is `NCT6687D` [V: LHM LpcIO.cs].
- This board is "MAG B550 TOMAHAWK (MS-7C91)" [V-local], so it is **NCT6687D (non-DR)**.
- Linux `nct6687.c`: `SIO_NCT6687_ID 0xd590`, `SIO_ID_MASK 0xFFF0` [V].

**No dedicated PawnIO module exists.** Use the generic **LpcIO** [V: https://github.com/namazso/PawnIO.Modules/blob/0.2.11/LpcIO.p]:
- `ioctl_select_slot(1)` selects ports 0x4E/0x4F and clears the BAR list.
- `ioctl_find_bars` needs config mode entered and a chip ID that is neither 0x00 nor 0xFF. It walks LDNs 0..0xFE, reading registers 0x60 and 0x62 twice with a 1 ms gap, and keeps stable values ≥ 0x100 as 8-byte windows.
- `ioctl_pio_inb/outb` work only on:
  - the register-port pair;
  - the BAR windows (`port & 0xFFF8`);
  - 0x25C/0x25D.
  - PCI config ports 0xCF8–0xCFF are always blocked. Anything else returns `STATUS_ACCESS_DENIED`; no slot returns `STATUS_DEVICE_NOT_READY`.
- `ioctl_superio_inb/inw/outb` access configuration registers; `inw` = (reg<<8)|reg+1.
- Every call must hold `Global\Access_ISABUS.HTP.Method`.

**Whether the NCT6687D has a usable SMBus master:** unknown [U]. SignalRGB creates no Nuvoton bus on this PC [V-local], and ADR-0007 already lists it as possibly unsupported.

**Facts for M5-stretch fan and sensor ops.**
- Sources: LHM `Nct677X.cs`, `LpcIO.cs` and `LpcPort.cs` (MPL-2.0), and Linux out-of-tree `nct6687.c` (Fred78290, GPL-2.0-or-later). Both are used for **facts only**.
- If LHM code were ported, MPL-2.0 file-level copyleft would apply to those files only, which is acceptable inside a larger Apache/MIT work. The recommendation is still a clean-room Rust implementation from this fact table.

**Config mode:**
- Enter: write 0x87, 0x87 to 0x4E (via `ioctl_pio_outb`). Exit: write 0xAA. Linux additionally writes 0x02 to config register 0x02.
- Chip ID at registers 0x20/0x21; LDN select register 0x07; hardware-monitor LDN **0x0B**; base = word at 0x60/0x61 (0x0A20 here).
- Sources: [V: LHM LpcPort.cs/LpcIO.cs; nct6687.c `NCT6687_LD_HWM 0x0b`]

**EC-space access** via base+4 (page), base+5 (index), base+6 (data), i.e. ports 0xA24/0xA25/0xA26, all inside the 0xA20 BAR window [V: Nct677X.cs `ReadByte/WriteByte`; nct6687.c `EC_SPACE_*`]:
1. Wait until the page register reads 0xFF (LHM forces it after 500 ms; Linux writes 0xFF without waiting).
2. Write the page (address >> 8), write the index (address & 0xFF), then read or write data.
3. Write 0xFF to the page register to release it.
- All addresses below (0x100, 0xA00, 0xA28, 0xCF8, …) are **EC-space addresses**, not I/O ports. 0xCF8 here is page 0x0C, index 0xF8, not the PCI port.

**Registers** [V]:
- **Temperatures** (11): EC `0x100 + 2i`, i = 0..10 (LHM labels: CPU, System, MOS, PCH, CPU socket, PCIE_1, M2_1, …). Value = `(i8)EC[r] + 0.5 × ((EC[r+1] >> 7) & 1)`.
- **Voltages** (14, LHM order): `[0x120,0x122,0x124,0x126,0x128,0x12A,0x12C,0x12E,0x130,0x13A,0x13E,0x136,0x138,0x13C]`.
  - Value = `0.001 × (16 × EC[r] + (EC[r+1] >> 4))`.
  - Scale: index 0 (12 V) ×12, index 1 (5 V) ×5, index 4 (DRAM) ×2, others ×1.
  - Linux uses `0x120 + 2x` with its own table.
- **Fan RPM:** `0x140 + 2i` (i = 0..15), 16-bit `hi<<8 | lo`.
  - Extra register 0x852 (SYS Fan 7 on some boards) is a **13-bit count**: `count = (hi<<5) | (lo & 0x1F)`, and hi = 0xFF with lo = 0xF8 means no fan.
- **PWM readback:** `0x160 + i`, 0–255 (LHM shows `/2.55`).
- **Default MSI mapping** (Linux `nct6687_fan_config_default`, LHM non-DR):
  - CPU: rpm 0x140, pwm 0x160, write 0xA28
  - Pump: rpm 0x142, pwm 0x161, write 0xA29
  - SYS1–6: rpm 0x144–0x14E, pwm 0x162–0x167, write 0xA2A–0xA2F
  - The mode bit index equals the fan index.

**Manual PWM, LHM sequence** (non-DR) [V]:
1. Write `0x80` to EC 0xA01 (`NCT668X_FAN_CFG_REQUEST`) and wait 50 ms.
2. Read 0xA00 and set bit i.
3. Write the duty to `0xA28 + i`.
4. Write `0x40` to 0xA01 (`NCT668X_FAN_CFG_COMPLETE`) and wait 50 ms.

**Restore, LHM** [V]:
1. Write 0xA00 with bit i restored to its saved value.
2. Write 0x80 to 0xA01 and wait 50 ms.
3. Write the saved duty to 0xA28+i.
4. Write 0x40 to 0xA01 and wait 50 ms.

**More robust Linux handshake** [V facts: nct6687.c `start_fan_cfg_update`/`finish_fan_cfg_update`]:
- Uses EC register **0xCF8** (`NCT6687_REG_FAN_ENGINE_STS`): bit 6 LOCK, bit 3 PHASE, bit 4 INVALID, bit 5 CHECK_DONE.
- **Start:**
  1. Wait until PHASE is clear and bit 7 of 0xA01 is clear.
  2. Write 0x80 to 0xA01.
  3. Wait until LOCK is clear and PHASE is set.
- **Finish:**
  1. Write 0x40 to 0xA01.
  2. Wait for CHECK_DONE.
- Each wait is up to 1 s.

**Init (LHM):**
- Set bit 7 of EC 0x180 if it is clear.
- Write 0x61–0x65 to EC 0x1BB–0x1BF ("enable SIO voltage").
- These writes are needed only for voltages.

**Not verified:**
- Header mapping on the MAG B550 TOMAHAWK [U].
- Whether the 0xCF8 handshake applies to this chip [U].
- Coexistence with SignalRGB fan control. It is active on this PC, so Lightloom must not drive fans while SignalRGB runs [V-local: "Motherboard fan control enabled: true"].

---
## 4. NVIDIA GPU I2C (MSI RTX 3060, 10DE:2504, subsystem 1462:3903, at 0x68)
**Headers.** https://github.com/NVIDIA/nvapi (HEAD 70d337d).
- `nvapi.h`, `nvapi_interface.h` and `nvapi_lite_common.h` carry `SPDX-License-Identifier: MIT` (© 2019–2026 NVIDIA). The README says "licensed under the MIT License", and `License.txt` (MIT) covers the `.lib` files.
- The headers may be vendored with attribution [V].

**DLL** [V-local]:
- `C:\Windows\System32\nvapi64.dll`, FileVersion 32.0.15.8115 (driver 581.15; WMI GPU DriverVersion 32.0.15.8115).
- Authenticode valid; signer "Microsoft Windows Hardware Compatibility Publisher".
- Exports **only** `nvapi_Direct_GetMethod` and `nvapi_QueryInterface` (PE export table).
- Every other function is resolved with `void* nvapi_QueryInterface(u32 id)`. A bogus id returns NULL.

**Interface ids** [V: nvapi_interface.h]; all resolve non-NULL on 581.15 [V-local; no function called]:

| Function | ID |
|---|---|
| NvAPI_Initialize | 0x0150E828 |
| NvAPI_Unload | 0xD22BDD7E |
| NvAPI_GetErrorMessage | 0x6C2D048C |
| NvAPI_EnumPhysicalGPUs | 0xE5AC921F |
| NvAPI_GPU_GetFullName | 0xCEEE8E9F |
| NvAPI_GPU_GetPCIIdentifiers | 0x2DDFB66E |
| NvAPI_GPU_GetBusId | 0x1BE0B8E5 |
| NvAPI_I2CRead | 0x2FDE12C5 |
| NvAPI_I2CWrite | 0xE812EB07 |
| NvAPI_SYS_GetDriverAndBranchVersion | 0x2926AAAD |
| NvAPI_GPU_GetThermalSettings | 0xE3640A56 |
| **NvAPI_I2CReadEx** (community; not in NVIDIA headers) | **0x4D7B0709** |
| **NvAPI_I2CWriteEx** (community; not in NVIDIA headers) | **0x283AC65A** |

- The Ex ids come from OpenRGB `dependencies/NVFC/nvapi.cpp`, which declares `NV_STATUS (*)(NV_PHYSICAL_GPU_HANDLE, NV_I2C_INFO_V3*, NV_U32* unknown)` [V as OpenRGB usage; U as NVIDIA API].

**Signatures and ABI.**
- `NVAPI_INTERFACE = extern NvAPI_Status __cdecl` [V: nvapi_lite_salstart.h], i.e. Rust `extern "C"`. Headers are under `#pragma pack(push,8)`.
- `NvAPI_Initialize()` and `NvAPI_Unload()` are ref-counted and "must be called in pairs".
- `NvAPI_EnumPhysicalGPUs(NvPhysicalGpuHandle h[64], NvU32* count)`
- `NvAPI_GPU_GetPCIIdentifiers(h, NvU32* DeviceId, NvU32* SubSystemId, NvU32* RevisionId, NvU32* ExtDeviceId)`. The packing is `DeviceId = dev<<16 | ven` and `SubSystemId = subdev<<16 | subven`, as decoded by OpenRGB [V usage; the header does not specify it].
- `NvAPI_GPU_GetFullName(h, char[64])`
- `NvAPI_I2CRead/Write(h, NV_I2C_INFO*)`

**Public I2C limits** [V: nvapi.h]:
- The public `NvAPI_I2CRead/Write` "must be for a DDC port: pI2cInfo->bIsDDCPort = 1".
- "A data buffer size larger than 16 bytes may be rejected if a register address is specified" (→ `NVAPI_ARGUMENT_EXCEED_MAX_SIZE` −116).
- Read with a register uses the combined format (write register, then repeated-start read). Write with a register sends the register "immediately follow[ed] without a restart".
- `NVAPI_MAX_SIZEOF_I2C_DATA_BUFFER` 4096; `NVAPI_MAX_SIZEOF_I2C_REG_ADDRESS` 4.

**`NV_I2C_INFO_V3`** (x64): size **64**; `NV_I2C_INFO_VER3 = 0x00030040`, since `MAKE_NVAPI_VERSION = sizeof | ver<<16` [V: nvapi.h; offsets recomputed with ctypes]:

| Offset | Field |
|---|---|
| 0 | `u32 version` |
| 4 | `u32 displayMask` |
| 8 | `u8 bIsDDCPort` |
| 9 | `u8 i2cDevAddress` (7-bit address << 1) |
| 16 | `u8* pbI2cRegAddress` |
| 24 | `u32 regAddrSize` |
| 32 | `u8* pbData` |
| 40 | `u32 cbSize` |
| 44 | `u32 i2cSpeed` (must be `NVAPI_I2C_SPEED_DEPRECATED` 0xFFFF) |
| 48 | `i32 i2cSpeedKhz` (enum: DEFAULT 0, 3 kHz 1, 10 kHz 2, 33 kHz 3, 100 kHz 4, 200 kHz 5, 400 kHz 6) |
| 52 | `u8 portId` |
| 56 | `u32 bIsPortIdSet` |

**Values OpenRGB uses for GPU RGB** [V: https://gitlab.com/CalcProgrammer1/OpenRGB/-/blob/master/i2c_smbus/Windows/i2c_smbus_nvapi.cpp]:
- `bIsDDCPort = 0`, `portId = 1`, `bIsPortIdSet = 1` ("Always use GPU port 1 - this is where RGB controllers are attached").
- `i2cSpeed = 0xFFFF`, `i2cSpeedKhz = NVAPI_I2C_SPEED_DEFAULT`, `i2cDevAddress = addr << 1` (0x68 → 0xD0), `unknown = 0`.
- **SMBus mapping:**
  - BYTE: no register, size 1; a write sends `command` as the data byte.
  - BYTE_DATA: 1-byte register, size 1.
  - WORD_DATA: register, size 2, LSB first.
  - BLOCK_DATA: register, payload `[count, data…]` of size count+1.
  - I2C_BLOCK_DATA: register, payload `data`.
  - Raw `i2c_xfer`: register NULL, size n.
  - QUICK: returns −1.
- No mutex is used for NvAPI.
- Detection calls `NvAPI_Initialize`, `EnumPhysicalGPUs` and `GetPCIIdentifiers`.

**Status codes** [V: nvapi_lite_common.h]: 0 OK, −1 ERROR, −2 LIBRARY_NOT_FOUND, −3 NO_IMPLEMENTATION, −4 API_NOT_INITIALIZED, −5 INVALID_ARGUMENT, −6 NVIDIA_DEVICE_NOT_FOUND, −7 END_ENUMERATION, −8 INVALID_HANDLE, −9 INCOMPATIBLE_STRUCT_VERSION, −10 HANDLE_INVALIDATED, −101 EXPECTED_PHYSICAL_GPU_HANDLE, −104 NOT_SUPPORTED, −105 PORTID_NOT_FOUND ("No port ID was found for the I2C transaction"), −108 DEVICE_BUSY, −116 ARGUMENT_EXCEED_MAX_SIZE, −191 TIMEOUT.
- There is **no I2C NACK status**; no code exists in −30..−39.

**What the MSI plugin needs.** SignalRGB actually loads the **CDN copy** `%USERPROFILE%\AppData\Local\WhirlwindFX\SignalRgb\cache\plugin_cdn\main\Plugins\Msi\GPU's\MSI_GPU.js`; the log says "Plugin:MSI_GPU.js@92a39a42:CDN". It differs from the bundled `…\VortxEngine\app-2.5.80\Signal-x64\Plugins\Msi\GPU's\MSI_GPU.js` [V-local]. From the CDN copy [V-local]:
- `Scan` gates on `bus.IsNvidiaBus()` and matches Vendor, SubVendor, Product and SubDevice. 0x2504 is `RTX3060_LHR`, MSI subsystem 0x3903; address 0x68.
- Registers: BRIGHTNESS 0x36, SPEED 0x38, START 0x26, R1/G1/B1 0x30/0x31/0x32, R2–B3 0x27–0x2C, MODE 0x22.
- `initializeGPU` writes 0x2E=0x00, sets STATIC mode and brightness 0x64.
- `Render` → `CheckedWrite` R1, G1, B1, each with up to **4 attempts** of `WriteByte`, then `device.pause(120)`.
- At startup it uses `ReadByte` on R1, G1, B1, BRIGHTNESS and MODE.
- No `WriteBlock`. **Only SmbusReadByte and SmbusWriteByte are needed.**

**Privilege.**
- Non-elevated user process works [V-local, indirect]:
  - SignalRgb.exe (Limited token) logs "Opened NvApi! … Nvidia driver version: 581.15 … Adding NvGPU" (10:20:47);
  - it logs "Found SMBus device for bus: [NVIDIA GeForce RTX 3060] Address: [104]" and "Found Supported GPU … 0x2504 … 0x3903, Plugin:MSI_GPU.js@92a39a42:CDN" (10:21:11);
  - there are no NvApi read or write error lines in any of the 10 local logs;
  - two "NVAPI re-initialized after system resume (status 0)" events appear.
- LocalSystem / session 0: [U].
- SignalRgb.exe also contains `nvapi_pepQueryInterface`, "Failed to unlock NvAPI! Error Code: {}" and "Failed to Initilize Private NvApi Functions" [V-local]. Whether any private "unlock" is needed for I2C on this driver is [U]; OpenRGB's Ex path uses none.

**Rust plan** (`lightloom-nvapi`, MIT/Apache):
1. Load `nvapi64.dll` with `LoadLibraryExW(…, LOAD_LIBRARY_SEARCH_SYSTEM32 = 0x00000800)` [V: SDK libloaderapi.h; used successfully here] and get `nvapi_QueryInterface`.
2. Resolve the ids above.
3. Call `NvAPI_Initialize` once.
4. Enumerate GPUs and read the PCI 4-tuple (expect `0x250410DE` / `0x39031462`) and the full name ("NVIDIA GeForce RTX 3060", which becomes the bus `Name()`).
5. Serialise calls on one thread.
6. On −4 or −10, re-initialise and re-enumerate.
7. Call `NvAPI_Unload` at exit.

The repo stub `crates/lightloom-nvapi/src/lib.rs` has `#![forbid(unsafe_code)]`, which FFI will need relaxed, and an invalid placeholder `I2cNack = -39`.

Existing crates `nvapi` 0.1.4 (2022) and `nvapi-sys` 0.1.3 (2020), both MIT, are stale [V: crates.io]; write a minimal FFI:
```rust
type QueryInterfaceFn = unsafe extern "C" fn(id: u32) -> *mut c_void;
type InitializeFn = unsafe extern "C" fn() -> i32;            // also UnloadFn
type EnumPhysicalGpusFn = unsafe extern "C" fn(h: *mut NvPhysicalGpuHandle /*[64]*/, count: *mut u32) -> i32;
type GetPciIdentifiersFn = unsafe extern "C" fn(h: NvPhysicalGpuHandle, dev: *mut u32, subsys: *mut u32, rev: *mut u32, ext: *mut u32) -> i32;
type GetFullNameFn = unsafe extern "C" fn(h: NvPhysicalGpuHandle, name: *mut c_char /*[64]*/) -> i32;
type I2cExFn = unsafe extern "C" fn(h: NvPhysicalGpuHandle, info: *mut NvI2cInfoV3, unknown: *mut u32) -> i32; // ReadEx/WriteEx
#[repr(transparent)] #[derive(Clone, Copy)] pub struct NvPhysicalGpuHandle(*mut c_void);
#[repr(C)] pub struct NvI2cInfoV3 { version: u32, display_mask: u32, is_ddc_port: u8, i2c_dev_address: u8,
    reg_address: *mut u8, reg_addr_size: u32, data: *mut u8, size: u32, i2c_speed: u32, i2c_speed_khz: i32,
    port_id: u8, is_port_id_set: u32 }
const _: () = assert!(core::mem::size_of::<NvI2cInfoV3>() == 64);
```
Pointers must outlive the call, and `size` must not exceed the buffer.

---
## 5. AMD ADLX GPU I2C (low priority)
**The licence is not MIT.** The README says: "ADLX SDK is distributed under the license included in ADLX SDK License Agreement.pdf" [V: https://github.com/GPUOpen-LibrariesAndSDKs/ADLX, HEAD 32b5a74; PDF read]. Its terms:
- §1.5: "Licensed Materials" include "(d) header files".
- §2(c): distribution "solely in Object Code form, Sample Code and Derivative Works as incorporated in Your Software … subject to an end user license agreement".
- §3: the end-user licence must include "(b) prohibitions on reverse engineering" and "(f) AMD is a third party beneficiary".
- §4(b): you may not "distribute, publish … the Licensed Materials".
- §4(f): you may not "use the Licensed Materials in way that requires that the Licensed Materials or any portion thereof be licensed under a Free Software License".
- §12: confidentiality.
- **Result: the headers cannot be vendored into Apache/MIT source.**
- §7: "AMD software components (e.g. libraries, sample code) … licensed … under … their respective licenses located in the directory with the software component."

**Rust sample crate.** `Samples/rust/amd-adlx` has an MIT `LICENSE` (© 2021–2025 AMD) and a `Cargo.toml` with `license = "MIT"`, `publish = true`, version 0.1.0. It is not on crates.io ("crate `amd-adlx` does not exist") [V]. Whether §7 makes it usable is a legal question [U].

**API** (ADLX 2.0.0 build 125; `ADLXVersion.h`) [V: SDK/Include/II2C.h, ADLXDefines.h, ISystem.h, ADLX.h]:
- DLL `amdadlx64.dll`; cdecl entry points `ADLXQueryFullVersion`, `ADLXQueryVersion`, `ADLXInitialize`/`ADLXInitialize2`, `ADLXTerminate`.
- `IADLXSystem::GetGPUs` and `IADLXSystem::GetI2C(IADLXGPU*, IADLXI2C**)`.
- `IADLXI2C` (`ADLX_STD_CALL`):
  - `Version(int*, int*)`
  - `IsSupported(ADLX_I2C_LINE, int address, bool*)`
  - `Read`, `RepeatedStartRead` and `Write`, all `(line, speed /*KHz*/, address, offset, dataSize, data)`.
  - The address is documented as "The 7-bit I2C slave device address which is shifted one bit to the left".
- `ADLX_I2C_LINE`: OEM = 1, OD_CONTROL = 2, OEM2 = 3, OEM3 = 4, OEM4 = 5, OEM5 = 6, OEM6 = 7.

**Other implementations.**
- SignalRGB uses ADLX for AMD GPU I2C (log: "Failed to get AMD GPU list for I2C: Failed to initialize ADLX: General failure"; there is no AMD GPU here) [V-local].
- OpenRGB uses legacy ADL: `atiadlxx.dll` `ADL2_Display_WriteAndReadI2C` with `iLine = 1` ("location of the Aura chip"), `iSpeed = 100`, `iAddress = addr<<1` [V: OpenRGB i2c_smbus_amdadl.cpp].
  - Reads: `iOffset = command`, `iDataSize = n`.
  - Writes: `iOffset = 0` and the **command is prepended to the data** (`iDataSize = n+1`).

**Plan:**
- BusKind `AmdGpu`; daemon-side if it works unelevated [U].
- ReadByte/WriteByte map to Read/Write with offset = register and size 1. ReadBlockBytes maps to `RepeatedStartRead`/`Read`.
- WriteBlock count-byte semantics and WriteQuick emulation are [U]. SignalRGB's ASRock plugin does `WriteQuick(0x36)` on the ADLX bus [V-local].
- Do this after M5.

---
## 6. Mapping to lightloom-iobroker ops (`crates/lightloom-protocol/src/broker.rs`)
**Current protocol** [V: broker.rs]:
- `BROKER_PROTOCOL_VERSION = 1`, `MAX_BLOCK_LEN = 64`, `MAX_FRAME_BYTES = 4096`.
- Address range 0x03..=0x77. SPD deny ranges 0x50..=0x57 and 0x30..=0x37.
- `BusKind` {System, Nvidia, AmdGpu, Nuvoton, Intel, Amd}.
- Ops: `SmbusWriteQuick`, `SmbusReadByte`, `SmbusWriteByte`, `SmbusReadWord`, `SmbusWriteWord`, `SmbusWriteBlock`, `SmbusReadBlock`, `SmbusWriteBlockNoReg`, `SmbusReadBlockNoReg`, `SmbusReadByteNoReg`, `ListBuses`, `Health`.
- `BrokerError` {UnsupportedVersion, UnknownBus, AddressOutOfRange, AddressDenied, LengthOutOfRange, Nack, Timeout, Unsupported, RateLimited, DryRun, Backend{status: i32, message}, BadRequest}.

### 6.1 Buses on this PC
IDs come from PnP and names from SignalRGB's log [V-local]. The `max_block_len` values are proposed: SMBus maximum is 32, the largest single WriteBlock in the plugin corpus is 32, and the PIIX4 module rejects more than 32.

| id | name | kind | vendor:device | sub_vendor:sub_device | max_block_len | backend |
|---|---|---|---|---|---|---|
| 0 | `AMD PIIX4 Bus - 0x0b00` | Amd | 1022:790B | 1022:790B | 32 | PawnIO handle A, port 0 |
| 1 | `AMD PIIX4 Bus - 0x0b20` | Amd | 1022:790B | 1022:790B | 32 | PawnIO handle B, port 1 |
| (daemon) | `NVIDIA GeForce RTX 3060` | Nvidia | 10DE:2504 | 1462:3903 | 32 (public API: >16 with register may give −116; Ex limits [U]) | NvAPI port 1 |

### 6.2 Op mapping and capability matrix
Chipset column: `ioctl_smbus_xfer`, in = 9 / out = 5, zero-filled.

| BrokerOp | PIIX4 / I801 | NvAPI (Ex) | SignalRGB parity |
|---|---|---|---|
| SmbusWriteQuick{addr} | `[addr, 0, 0, 0]` | **Unsupported** | exe: "No quick writes, compadre." |
| SmbusReadByte{reg} | `[addr, 1, reg, 2]` → `out[0] & 0xFF` | ReadEx(reg[1], size 1) | ✓ |
| SmbusWriteByte{reg, v} | `[addr, 0, reg, 2, v]` | WriteEx(reg[1], [v]) | ✓ |
| SmbusReadWord{reg} | `[addr, 1, reg, 3]` → `out[0] & 0xFFFF` | ReadEx(reg, size 2), LSB first | ✓ |
| SmbusWriteWord{reg, w} | `[addr, 0, reg, 3, w]` | WriteEx(reg, [lo, hi]) | ✓ |
| SmbusWriteBlock{reg, data} | `[addr, 0, reg, 5, packed(len, data…)]` (SMBus block write with count byte) | WriteEx(reg, [len, data…]) as OpenRGB does; SignalRGB semantics [U] | ✓ |
| SmbusReadBlock{reg, len} (= ReadBlockBytes) | **Refuse** (parity). Modules only offer the count-prefixed SMBus block read, with different PIIX4/I801 layouts; I801 also has I2C_BLOCK_DATA. | ReadEx(reg, size len) | exe: "no Read Block Bytes outside of NvApi" |
| SmbusWriteBlockNoReg / SmbusReadBlockNoReg | **Refuse** | Write/ReadEx(reg NULL, size n) | exe: "no … without Register outside of NvApi" |
| SmbusReadByteNoReg | **Refuse by parity policy.** The module itself supports receive-byte via `[addr, 1, 0, 1]` → `out[0] & 0xFF`. | ReadEx(reg NULL, size 1) | exe: "no ReadByteWithoutRegister … outside of NvApi" |
| ListBuses / Health | from the handles; Health adds driver version, edition, pins, CI flags | — | — |

### 6.3 Error mapping (NTSTATUS → BrokerError)
- `0xC000000E STATUS_NO_SUCH_DEVICE` → `Nack`.
- `0xC00000B5 STATUS_IO_TIMEOUT`, or a mutex wait timeout → `Timeout` (or a new `BusContended`).
- `0xC00000BB STATUS_NOT_SUPPORTED`, or an op outside the capability matrix → `Unsupported`.
- These go to `Backend{status, message}`:
  - `0x80000011 STATUS_DEVICE_BUSY` (retryable);
  - `0xC0000185 STATUS_IO_DEVICE_ERROR`;
  - `0xC0000186 STATUS_DEVICE_PROTOCOL_ERROR`;
  - `0xC0000022 STATUS_ACCESS_DENIED` (LpcIO port allowlist);
  - `0xC00000A3 STATUS_DEVICE_NOT_READY`;
  - `0xC0000034 STATUS_OBJECT_NAME_NOT_FOUND`;
  - `0xC000000D STATUS_INVALID_PARAMETER`;
  - `0xC0000023 STATUS_BUFFER_TOO_SMALL`.
  - All values from SDK `ntstatus.h` [V].
- The JS wrapper returns **−1** for any chipset error (SignalRGB parity) and passes **NvAPI status codes** through on GPU buses.

### 6.4 Policy (static, enforced in the broker)
- **Keep:** SPD write denial, 2000 ops/s (ADR-0004), dry-run, audit log.
- **Scope the SPD deny to chipset buses** (System, Amd, Intel). The ASRock GPU plugin does `WriteQuick(0x36)` on the AMD GPU (ADLX) bus [V-local], and the current `touches_spd()` is bus-agnostic [V: broker.rs].
- **Known conflict:** `Patriot_Viper_RAM.js` does `WriteByte(0x36, 0x00, 0xff)` and `WriteByte(0x37, 0x00, 0xFF)` (DDR4 SPD page select) [V-local], so it needs an explicit, audited per-plugin override.
- **Add** [U]:
  - deny writes to DDR5 PMIC (0x48–0x4F) and RCD (0x58–0x5F) when the RAM is DDR5 (this PC: DDR4, SMBIOS type 26 [V-local]);
  - allow only PIIX4 ports {0, 1};
  - allow no raw port, MSR or SMN on the wire.
- **Allowlist caveat:** `Scan()` itself writes. Crucial does WriteQuick sweeps of 0x39–0x3C and 0x20–0x23 and remaps via 0x27 registers 0x82EE/0x82EF/0x82F0; ENE remaps from 0x77 [V-local]. So "Scan()-derived allowlists" cannot gate scanning. Use a scan window with the deny ranges and quick-write only, then restrict the run phase to the addresses that were discovered.

### 6.5 Proposed protocol additions (BROKER_PROTOCOL_VERSION → 2) [U design]
- **`BusLock { bus, lease_ms: u16 }` / `BusUnlock { bus }`.** The broker holds `Access_SMBUS` until unlock or until the lease expires (default 50 ms, maximum 250 ms; on expiry it releases the mutex and audits). This maps to `TransactionLock`.
  - Crucial per DIMM per frame [V-local: `Crucial Ballistix Ram.js`]: `TransactionLock`, then 3 × (`WriteWord(0x00, bswap16(reg))` + `WriteBlock(0x03, 8, data)`) for 0x8300/0x8340/0x8380, then `TransactionUnlock`. That is 6 SMBus transactions per DIMM and 12 per frame, about 360/s at an assumed 30 fps.
  - OpenRGB's Crucial controller uses the same sequence with `i2c_smbus_write_block_data(dev, 0x03, 8, data)` (SMBus block write with count byte), falling back to byte writes at 0x01 [V: OpenRGB CrucialController.cpp].
- **Optional `SmbusBatch { bus, addr, steps ≤ 16 }`** for write-only locked regions.
- **SuperIO, closed and high-level (M5 stretch):**
  - Ops: `SioIdentify → {slot, chip_id, rev, hwm_base}`, `SioSensors → {temps, fans_rpm, pwm, volts}`, `FanSetDuty { fan: 0..7, duty }`, `FanRestoreAuto { fan }`.
  - EC writes limited to 0xA00, 0xA01 and 0xA28–0xA2F, plus 0x180 and 0x1BB–0x1BF if voltages are wanted. EC reads include 0xCF8 if the Linux handshake is used.
- **`CpuTemp`** through AMDFamily17 `ioctl_read_smn` under `Access_PCI`. The module checks that PCI 0:0:0 vendor is 0x1022 and uses SMN index/data at config 0x60/0x64 [V].
  - Decode (facts from Linux `drivers/hwmon/k10temp.c`, GPL; facts only) [V]:
    - Tctl at SMN `0x00059800`: °C = `(reg >> 21) × 0.125`.
    - Subtract 49 °C if bit 19 is set or bits 17:16 == 0b11.
    - Ryzen 5 5600X = family 0x19, model 0x21 ("AMD64 Family 25 Model 33" [V-local]), which has no Tctl offset (the offset table lists only some Zen 1/Zen+ parts).
    - CCD0 temperature at SMN `0x00059800 + 0x154` = `0x00059954`: valid bit 11, °C = `(reg & 0x7FF) × 0.125 − 49`.
  - Runtime read on this CPU: [U].
- **`HealthReport` additions:** `pawnio_driver_version: u32`, `pawnio_edition: Official | Unrestricted | Unknown`, `ci_options: u32`, `sleep_mode`.

---
## 7. Install and packaging steps (lightloom-setup, one elevated run)
1. **Check PawnIO.**
   - Read `HKLM\…\Uninstall\PawnIO` (`DisplayVersion`, `InstallLocation`).
   - Open `\\?\GLOBALROOT\Device\PawnIO` and query `IOCTL_PIO_VERSION` (`STATUS_NOT_IMPLEMENTED` means 2.0.x).
   - Query CI options (TESTSIGN, HVCI).
2. **If PawnIO is missing or older than 2.2.0:** after consent (show "PawnIO is a third-party driver; binary edition proprietary freeware, source GPL-2.0-or-later; https://pawnio.eu"):
   - download the 2.2.0 installer and verify SHA-256 `1F519A22…3032` (plus the Authenticode signature [U signer]);
   - run `PawnIO_setup.exe -install -silent` and accept 0 and 3010 (3010 = reboot needed; start the broker after reboot).
   - Alternatives: `winget install --id namazso.PawnIO -e --version 2.2.0 --silent --accept-package-agreements --accept-source-agreements`; or a user-supplied installer file.
   - Do not embed the installer in the MSI. Redistribution is allowed and practised by FanControl and LHM, but ADR-0007 forbids shipping a `.sys` in the installer.
3. **Install the blobs.**
   - Copy the pinned blobs to `%ProgramFiles%\Lightloom\pawnio\0.2.11\`.
   - Add `COPYING` (LGPL-2.1), the matching `.p` sources, `sleepmode.inc` and `include/`, and `SOURCE.txt` (https://github.com/namazso/PawnIO.Modules/tree/0.2.11).
   - ACL: write for SYSTEM and Administrators only; read for Users.
4. **Install the `LightloomIoBroker` service:**
   - LocalSystem, automatic start, failure actions = restart with backoff.
   - Set `SERVICE_CONFIG_REQUIRED_PRIVILEGES_INFO` to the minimum. Docs: "the SCM will remove the privileges that are not required from the process token when the process starts" and "SeChangeNotifyPrivilege privilege is never removed" [V: learn.microsoft.com SERVICE_REQUIRED_PRIVILEGES_INFOW]. Add `SeImpersonatePrivilege` only if the pipe server impersonates.
   - Use `SERVICE_SID_TYPE_UNRESTRICTED`.
     - [U] `SERVICE_SID_TYPE_RESTRICTED` makes a write-restricted token, which would fail a write-access open of the SY/BA-only device.
     - Because PawnIO IOCTLs are `FILE_ANY_ACCESS`, a read-only open might still work; this is untested.
   - Call `SetDefaultDllDirectories(LOAD_LIBRARY_SEARCH_SYSTEM32)`.
5. **Pipe:** DACL limited to the recorded interactive user, `PIPE_REJECT_REMOTE_CLIENTS`, a per-boot token (ADR-0004).
6. **Uninstall:** stop and delete the service and remove the blobs. **Never uninstall PawnIO automatically.**

---
## 8. Risks, open questions, repo-doc corrections
- **R1, sleep-mode latency** [U estimate]. AlwaysSleep uses `KeDelayExecutionThread`, so every wait costs at least the timer resolution. With 12 transactions per frame that is roughly 12–190 ms, versus about 9 ms of bus time when busy-waiting at 93.75 kHz. OpenRGB defaults to AlwaysSleep. Benchmark modes 0, 1 and 2; default to 0 or 1 for RAM RGB.
- **R2, undocumented NvAPI Ex functions and a possible private "unlock" path** (SignalRGB strings). Resolve the ids at start-up and degrade gracefully.
- **R3, module/driver ABI drift.**
  - Block packing changed in 2025-09 (commit ba57344, "Pack block data into a single array"; in tags 0.1.6 and later).
  - I801 block-process layout was fixed in 0.2.11 (3f5eddc).
  - PIIX4 and I801 block reads differ.
  - Mitigation: pin hashes and run a per-module conformance test.
- **R4, HVCI strict mode is on here;** PawnIO under HVCI is untested [U].
- **R5, mutex DACL interop and squatting** (§1.11).
- **R6, other tools moving the PIIX4 primary mux;** re-select port 0 per batch.
- **R7, a compromised daemon** can write any non-denied address. Keep the static policy and the audit log.
- **R8, anti-cheat interplay.** FanControl V259 notes a FACEIT issue fixed with PawnIO 2.1.0 [V]; behaviour with 2.2.0 is [U].
- **R9, SignalRGB coexistence.** It holds the SMBus, ISA and PCI mutexes and drives fans and GPU I2C on this PC. Lightloom must not run its buses or fan control concurrently with SignalRGB.
- **Open questions:**
  - NvAPI in session 0.
  - SignalRGB `WriteBlock` count-byte semantics. This is very likely an SMBus block write on PIIX4: the same device under OpenRGB uses `write_block_data`.
  - NCT6687D header mapping on this board and whether the chip has an SMBus master.
  - Whether `-install -silent` always picks the official edition.
- **Repo corrections:**
  - (a) `docs/host-api/smbus.md` (critic gap at line 226): the NCT6687D is at **0x4E**; 0x2E is empty. The ID check also reads 0xFFFF on 0x2E.
  - (b) ADR-0004 line 46 and plan line 8: Pawn modules must be **LGPL-2.1 and upstreamed** to be signed. A "separate GPL-2 repository" cannot load on official builds.
  - (c) `docs/architecture-synthesis.md`:
    - line 44: the path should be `\\?\GLOBALROOT\Device\PawnIO`, not `\\.\PawnIO` (absent on 2.1.0, deprecated on 2.2.0);
    - line 84: "ADLX MIT" is wrong;
    - line 84 also says "no NVIDIA headers", but the NVIDIA nvapi headers are MIT and may be vendored.
  - (d) ADR-0004 line 49: consider moving NvAPI to `lightloomd` (§0 item 5).
  - (e) Per-bus `max_block_len` = 32 (wire `MAX_BLOCK_LEN` 64 is fine).
  - (f) `crates/lightloom-nvapi/src/lib.rs`: `I2cNack = -39` does not exist in NvAPI. Use `NVAPI_ERROR`/−105 `PORTID_NOT_FOUND`; −8 `INVALID_HANDLE` is correct.
  - (g) `broker.rs` `touches_spd()` must be BusKind-aware.

## 9. M5 spike checklist (in order)
1. Install PawnIO 2.2.0 under HVCI strict mode. Check version `0x00020200` and CI options (TESTSIGN off).
2. Run the Echo tamper probe (expect the load to fail), then the genuine Echo `ioctl_not`.
3. Load PIIX4 on handles A and B and call `port_sel` 0 and 1 under the mutex. `ioctl_identity` should return `0x0B00`/`0x0B20` and `0x790B1022790B1022`.
4. Quick-write 0x39 and 0x3A under the mutex; measure latency in sleep modes 0, 1 and 2.
5. NvAPI `I2CReadEx` of 0x68 register 0x22, from an unelevated process and from a LocalSystem service.
6. LpcIO on slot 1:
   1. `pio_outb(0x4E, 0x87)` twice.
   2. Check `superio_inw(0x20) == 0xD592`.
   3. `find_bars`.
   4. Select LDN 0x0B and check base `0x0A20`.
   5. Exit config mode (`pio_outb(0x4E, 0xAA)`).
   6. Read EC 0x100 via 0xA24/0xA25/0xA26 (CPU temperature).
7. AMDFamily17 `ioctl_read_smn(0x00059800)` under `Access_PCI`; compare with LHM.
8. 30-minute soak with HWiNFO, OpenRGB and LHM running and SignalRGB closed; target zero `STATUS_IO_DEVICE_ERROR`.

---

## Fact-check corrections applied to the first draft

- §1.4 version floor: WRONG claim that get_proc_address/invoke natives are absent in PawnIO 2.0.0. The 2.0.0 native table (PawnIO/vm.cpp @2.0.0) contains both. Parsing the AMX native-import tables of the 0.2.11 SmbusPIIX4/SmbusI801/SmbusNCT6793/LpcIO/AMDFamily17/Echo blobs shows every import exists in 2.0.0, 2.1.0 and 2.2.0. The ≥2.2.0 floor is kept for other verified reasons: RS5/19H1/19H2 BSOD fix, INF/PnP security, IOCTL_PIO_VERSION from 2.1.0, PCI-native fixes in 2.1.0, FanControl's FACEIT note.
- §1.2 IOCTLs: '2.0.0 has the same values' is only partly true. 2.0.0 has LOAD (0xA1B22084) and EXECUTE (0xA1B22104) but no IOCTL_PIO_VERSION; it has IOCTL_PIO_GET_REFCOUNT (fn 0x801 = 0xA1B22004) instead. On 2.0.x a VERSION request returns STATUS_NOT_IMPLEMENTED (0xC0000002, the default in dispatch_irp).
- §1.2 device path history added: \\.\PawnIO existed in 2.0.0, was absent in 2.1.0, and was restored as deprecated in 2.2.0. In 2.2.0 \Device\PawnIO is a symlink to the PnP PDO; in 2.0/2.1 it was a legacy named device created with IoCreateDeviceSecure. Only \\?\GLOBALROOT\Device\PawnIO works across all versions.
- §1.6 PIIX4 main(): most failures return STATUS_NOT_SUPPORTED (0xC00000BB): wrong arch, vendor, device, revision < 0x51, MMIO map failure, SMBus disabled, status register 0xFF. Only a non-0xB00 SMBus base returns STATUS_NOT_IMPLEMENTED; the brief implied NOT_IMPLEMENTED for all of them.
- §1.6 ioctl_clock_freq: the argument range is 10 kHz–1 MHz, but SMBTIMING must be 1..255, so the effective minimum is about 64.7 kHz (the module doc says 'minimum … ~64705Hz').
- §1.1 licences: include/registry.inc is LGPL-2.1-or-later, not 0BSD. LpcIO.p and LpcCrOSEC.p carry the LGPL-2.1-or-later text without an SPDX tag. sleepmode.inc has no header, so the repo COPYING (LGPL-2.1) applies.
- §0/§7 LGPL obligations: recommend shipping the matching .p sources, sleepmode.inc and include/ next to the blobs. LGPL-2.1 §4 asks for accompanying source or equivalent access from the same place; a bare URL is weaker (legal reading, unverified).
- §1.4 Unrestricted edition: pawnio.eu calls it 'An unsigned version without signature checking. Can be selected in the installer'; the wiki says 'only test signed'. Added a code-integrity check: NtQuerySystemInformation(SystemCodeIntegrityInformation=103), CODEINTEGRITY_OPTION_TESTSIGN 0x02 (SDK winternl.h). This PC returns 0xF401: TESTSIGN off, HVCI KMCI, strict mode and IUM on.
- §1.6 SmbusNCT6793: its main() writes 0x87,0x87 to port 0x2E at load time without any mutex and never exits config mode (no 0xAA). Do not load it. Also: Linux nct6687 uses SIO_ID_MASK 0xFFF0, not 0xFFF8.
- §1.5 AMDFamily17 exposes ioctl_read_msr and ioctl_write_msr as well as ioctl_read_smn. Writes are allow-listed: P-states, HWCR, CPPC, LS/IC/DC_CFG and others. The broker must never forward MSR on the wire.
- §1.7 PIIX4 port_sel(1) is not purely software. It runs piix4_busy_check() on the handle's current base, which may write the status register, and maps FCH MMIO. It must be called under the SMBus mutex.
- §1.10 winget: InstallerSuccessCodes lists only 3010 (0 is implicitly success); RepairBehavior is uninstaller. The 2.0.1 note 'If named "uninstall.exe", setup will behave as uninstaller' is now verified from the release page.
- §1.10 build flags: /INTEGRITYCHECK, /guard:ehcont and /CETCOMPAT are on the driver target in PawnIO/CMakeLists.txt @2.2.0, plus inf2cat /pageHashes. The top-level /INTEGRITYCHECK was removed in commit e1043d7 (user-mode targets only).
- §1.11 upgraded to primary sources: OpenRGB MR 1436 (GitLab API: merged 2022-10-14, 'to allow sharing access to the smbus with other applications like HWINFO, SIV'); WinRing0 Defender name HackTool:Win32/Winring0 (FanControl.Releases discussion #3017); CVE-2020-14979 text (MITRE CVE API).
- §1.11 added: SmbusIntelSkylakeIMC.p states HWiNFO and SIV use the SMBus mutex. SignalRgb.exe contains the literal Global\Access_SMBUS.HTP.Method and the access-denied message. A non-elevated SignalRGB created the Global mutexes on this PC, so Global mutex squatting by user processes is possible.
- §1.11 SignalRgbDriver.sys import scan: from HAL it imports only HalGetBusDataByOffset and HalSetBusDataByOffset (PCI config); there is no MmMapIoSpace and nothing I2C-related. This supports 'GPU I2C must go through NvAPI'.
- §1.9 LHM: MainForm.cs offers to install PawnIO when it is missing or older than 2.0.0 (not 2.2.0) and runs the bundled PawnIO_setup.exe with '-install' (not silent). LHM opens the device with FileAccess.ReadWrite (FILE_READ_DATA|FILE_WRITE_DATA).
- §1.8 OpenRGB ships its own PawnIOLib.dll copy and also LedsValve.bin. pawnio_version() only proves the bundled DLL loaded (hard-coded library version), not the driver. If CreateMutexA returns NULL, OpenRGB runs unlocked.
- §1.3 three flavours: execute_async is the exception. pawnio_execute_async (HRESULT) and _win32 take an OVERLAPPED, while pawnio_execute_async_nt has a different signature (event, APC, IOSB). 2.2.0 vs master differences noted: nt_to_hresult success mapping, length checks, 30 vs 31 characters.
- §1.2 close semantics: unload runs on IRP_MJ_CLOSE (last file-object reference). Added STATUS_DEVICE_NOT_READY/STATUS_OBJECT_NAME_NOT_FOUND/STATUS_INVALID_PARAMETER cases and the exact aliasing rule (output cell k aliases input cell k−4).
- §2 cargo feature names, previously [U], are now verified in windows 0.62.2's crates.io feature table; added Win32_Foundation and Win32_System_LibraryLoader. NtDeviceIoControlFile parameter types corrected to event: Option<HANDLE>, apccontext: Option<*const c_void>, etc. libloading 0.9.0 get() takes `symbol: impl AsSymbolName`.
- §4 LOAD_LIBRARY_SEARCH_SYSTEM32 = 0x00000800, previously unverified, is now verified in SDK libloaderapi.h and used successfully on this PC.
- §3 voltages: the 14 registers are not plain 0x120+2i. LHM order is [0x120,0x122,0x124,0x126,0x128,0x12A,0x12C,0x12E,0x130,0x13A,0x13E,0x136,0x138,0x13C], and index 4 (DRAM) is scaled ×2. The temperature half-degree bit is bit 7 of EC[reg+1]. Fan register 0x852 is a 13-bit count, not a 16-bit RPM.
- §3 EC addresses 0xA00/0xA01/0xA28+ are EC-space (page/index/data through 0xA24–0xA26), not I/O ports. The restore-to-auto sequence was marked unverified but is verified as LHM practice. Added the Linux EC handshake (engine status EC 0xCF8: LOCK/PHASE/INVALID/CHECK_DONE) and the init writes 0x180 and 0x1BB–0x1BF needed for voltages.
- §3 chip variant: LHM classifies ID 0xD5/rev 0x92 as NCT6687DR only on MSI B840/B850/B860/X870/Z890 boards; MAG B550 TOMAHAWK is NCT6687D (non-DR). The SuperIO evidence is 5 local logs with an LPC scan, not 6.
- §3/§8 SignalRGB has 'Motherboard fan control enabled: true' on this PC and drives the NCT6687D fans, which is a coexistence risk for Lightloom fan control.
- §4 MSI_GPU.js: the plugin SignalRGB actually loads is the CDN copy …\WhirlwindFX\SignalRgb\cache\plugin_cdn\main\Plugins\Msi\GPU's\MSI_GPU.js ('MSI_GPU.js@92a39a42:CDN'), which differs from the bundled VortxEngine copy. CheckedWrite makes up to 4 attempts in total (not 4 retries). initializeGPU also writes 0x2E=0x00. 0x2504 is the RTX3060_LHR id.
- §4 NvAPI status codes added: −7 END_ENUMERATION, −8 INVALID_HANDLE, −101 EXPECTED_PHYSICAL_GPU_HANDLE, −105 PORTID_NOT_FOUND. There is no I2C NACK status and no code in −30..−39, so the repo stub's I2cNack = −39 is invalid.
- §4 NvAPI access violation: this is a handler string in SignalRgb.exe, not an observed event; the 10 local logs show none, only 2 'NVAPI re-initialized after system resume (status 0)'. Added SignalRgb.exe strings nvapi_pepQueryInterface and 'Failed to unlock NvAPI!' as an open question.
- §4 GetPCIIdentifiers packing (dev<<16|ven) is verified only through OpenRGB usage; the NVIDIA header does not specify it.
- §5 ADLX licence: added §1.5(d) (header files are Licensed Materials), §2(c) (object-code-only distribution), §4(b) (no distribution of Licensed Materials) and §12 (confidentiality). Added that SignalRGB uses ADLX for AMD GPU I2C (local log). OpenRGB ADL: iOffset = command only for reads; writes prepend the command to the data (iDataSize = n+1).
- §6.2 SmbusReadByteNoReg: the PIIX4 and I801 modules do support receive-byte (I2C_SMBUS_BYTE read, [addr,1,0,1]). Refusing it is a SignalRGB-parity policy choice, not a module limitation.
- §6.4 the SPD deny must be scoped to chipset buses. The ASRock GPU plugin does WriteQuick(0x36) on the AMD GPU (ADLX) bus, and broker.rs touches_spd() is bus-agnostic. Patriot_Viper_RAM.js writes 0x36/0x37 (verified locally).
- §6.5 CPU temperature: the SMN Tctl register 0x00059800 and its decode, previously unverified, are now verified from Linux k10temp.c (facts only): °C = (reg>>21)×0.125, minus 49 if bit 19 is set or bits 17:16 == 0b11. 5600X = family 0x19 model 0x21 (verified locally), no offset. CCD0 temperature is at 0x00059954.
- §8 repo corrections expanded: architecture-synthesis line 84 'no NVIDIA headers' (the headers are MIT); plan line 8 GPL-2 repo; lightloom-nvapi placeholder status code; broker.rs SPD scoping. The earlier citation of 'SignalRGB access-denied mutex message' is now verified by an exe string scan.
- Added FanControl evidence: V259 'Support for PawnIO 2.1.0 driver (should fix FaceIT issue)'; V265 'Update included version of PawnIO to 2.2.0' (bundling precedent, anti-cheat risk).

## Still unverified after the fact-check

- NvAPI I2C (I2CReadEx/WriteEx) inside a LocalSystem service in session 0 is untested. Only non-elevated user-process operation is evidenced, indirectly, via SignalRgb.exe on this PC.
- NvAPI_I2CReadEx/WriteEx semantics (the third 'unknown' parameter, register-length limits, behaviour above 16 bytes) are community knowledge from OpenRGB NVFC. Only the ids 0x4D7B0709/0x283AC65A were verified to resolve on driver 581.15.
- Whether any private NvAPI 'unlock' path (SignalRgb.exe strings nvapi_pepQueryInterface, 'Failed to unlock NvAPI!') is required for GPU I2C writes on this driver.
- The exact count-byte semantics of SignalRGB's WriteBlock on PIIX4 and NvAPI. For Crucial on PIIX4 this is strongly suggested to be an SMBus block write with count byte, because OpenRGB drives the same device with write_block_data.
- PawnIO 2.2.0 loading and working with HVCI strict mode enabled (CI options 0xF401 on this PC). Only build flags suggest HVCI readiness.
- Whether the official PawnIO.sys is Microsoft attestation-signed; the installer's Authenticode signer; whether '-install -silent' installs the official (not Unrestricted) edition; the installer's uninstall CLI switches. The installer binary was not downloaded.
- The tampered-Echo probe and its expected STATUS_INVALID_SIGNATURE (designed, not executed); whether Unrestricted builds use the same device name and version resources.
- Runtime compatibility of the 0.2.11 modules with PawnIO 2.0.x/2.1.x. Only a static native-table check was done, and it passes.
- Runtime values on this PC: ioctl_identity 0x790B1022790B1022 and bases 0xB00/0xB20; whether the FCH PM MMIO checks (SMBus enable bit, base 0xB00) pass in SmbusPIIX4 main().
- Latency per PIIX4 transaction in each sleep mode (the 12–190 ms vs ~9 ms per-frame estimate), and SignalRGB's real frame rate (30 fps assumed).
- The exact default DACL of mutexes created by a LocalSystem process, and how non-elevated SignalRGB, elevated OpenRGB and HWiNFO would fail against it. Mutex squatting/DoS behaviour.
- Whether SERVICE_SID_TYPE_RESTRICTED can open the PawnIO device (for example read-only, since the IOCTLs are FILE_ANY_ACCESS), and the minimal required privilege set.
- That \\?\GLOBALROOT\Device\PawnIO works from a session-0 service (high confidence, untested).
- NCT6687D: fan-header mapping on the MSI MAG B550 TOMAHAWK; whether the chip has a usable SMBus master; whether the Linux EC handshake (EC 0xCF8) applies to this NCT6687D; the restore sequence's behaviour on this board.
- AMDFamily17 ioctl_read_smn of 0x00059800/0x00059954 at runtime on this Ryzen 5 5600X (the decode is verified from Linux source only).
- The DDR5 PMIC (0x48–0x4F) and RCD (0x58–0x5F) deny-list addresses (JEDEC knowledge, not fetched).
- ADLX: the correct ADLX_I2C_LINE for RGB controllers, speed, WriteQuick emulation, admin requirement, and whether the MIT amd-adlx sample crate is usable despite the ADLX SDK License Agreement (legal).
- Legal interpretations: distributing the LGPL module blobs as separate files with sources; bundling PawnIO_setup.exe versus ADR-0007; MPL-2.0 file-level obligations if any LHM code were ported.
- Whether WinRing0 is blocked by the Microsoft vulnerable-driver blocklist on this PC, and whether anti-cheat products (for example FACEIT) block PawnIO 2.2.0.
- Behaviour of other tools (HWiNFO and others) regarding leaving the PIIX4 primary port mux at a non-zero port; the per-batch port re-select is defensive.
- That moving NvAPI into lightloomd reduces attack surface without loss of capability (design reasoning).
