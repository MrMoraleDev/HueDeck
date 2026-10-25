<!-- SPDX-License-Identifier: GPL-3.0-or-later -->

# MSI graphics cards: per-LED ("direct") colours at I2C 0x68

Status: research only, static analysis. Nothing here has been sent to a card. The plugin
`plugins/msi-gpu/` still drives the card as one colour (registers `0x30..0x32`, see
`docs/hardware/msi-gpu.md`) unless its experimental, off-by-default "Per-LED" parameter is on
(§5.3); `lightloom probe msi-gpu-perled` runs the test of §5.2 (§5.4). This note covers the reference card, the MSI GeForce RTX 3060
Gaming X Trio LHR (`10DE:2504`, subsystem `1462:3903`), and what it would take to colour its
LEDs one by one.

Provenance tags as in `README.md`: **[V-local]** read from the installed MSI files on the
reference machine (static, nothing executed), **[U]** inference, **[D]** Lightloom design choice.

## Summary

**Result on the reference card (2026-10-25): the stream is ignored.** RTX 3060 Gaming X Trio LHR
(10DE:2504, 1462:3903), NvAPI port 1, 0x68. The identification reads gave LED type `0x0A`, module
type `0x0000`, firmware `0x11` and chip id `0xFFFF` (the chip-id register is not implemented). Two
runs of `lightloom probe msi-gpu-perled`:

1. Plain: `0x2E <- 1`, 34 single-slot frames, all red, all blue, a 6-slot short frame, `0x2E <- 0`.
   NvAPI acknowledged every transfer (status 0), including the 137-byte raw writes. The card kept
   showing the effect last saved in its flash by MSI Center and never changed.
2. `--static-first` (`0x36 <- 100`, `0x22 <- 0x13`, `0x26 <- 0`, then the same stream). The card
   went static white (colour 1), as in Lightloom's one-colour mode, and no frame changed it.

**Register dump (2026-10-25, reads only).** `lightloom probe msi-gpu-regs` first read every
register 0x00-0xFF once. Implemented registers: 0x22 = 0x20, 0x23-0x28 = 0, 0x29 = 0xE0,
0x2A = 0, 0x2B = 0xE0, 0x2C-0x2D = 0, 0x36 = 0x64, 0x37 = 0, 0x38 = 0x02, 0x39 = 0x02, 0x82 = 0x10,
0xEC = 0x0A. Every other register read 0xFF (unimplemented), 0x40 and 0xF0-0xFF failed. There is no
hidden per-LED or LED-count register, and 0x04/0x06 (the NV5000 stream) are unimplemented here.
**Caution:** after that sweep the LED controller answered nothing, neither reads nor writes,
until the PC was powered off at the supply. The probe now reads only the rows 0x20-0x3F,
0x80-0x8F and 0xE0-0xEF (`DUMP_ROWS`).

So this controller takes the per-LED stream on the bus and shows nothing of it. MSI's own
"SignalRGB bridge" sends that stream to every non-NV5000 card, which explains why it exists for
this card in MSI's code without working. The plugin refuses per-LED mode on LED type `0x0A`
(`IGNORES_STREAM_LED_TYPES`); other MSI cards with other controllers stay untried.


- **Channel: NvAPI I2C, address `0x68`** (8-bit `0xD0`), the same controller and bus as the
  one-colour path. Not the NVIDIA illumination API (that is used only for reference/Founders
  style zones, through command lines such as `--set_control --zoneidx …`), and not a separate
  ITE protocol: the "ITE" calls are two read-only identification reads on the same `0x68`.
  **[V-local], high confidence.**
- **Per-LED sequence** (MSI's own code for every non-"NV5000" NVIDIA card):
  1. enter: register `0x2E` ← `0x01`;
  2. each frame: one I2C write to register `0x49` of `4 × N` bytes, one 4-byte record per LED;
     MSI always sends N = 34 (136 bytes);
  3. leave: register `0x2E` ← `0x00` (the card goes back to its own effect **[U]**).

  No commit register is written per frame, and `0x3F` (save) is never written on this path.
  **[V-local], high confidence that this is what MSI sends; medium confidence that the
  firmware on this exact card honours it** (MSI does not branch on the card model here, only on
  "NV5000", so every RTX 30 Trio gets the same writes; nobody has watched the result yet).
- **Record layout:** `[c2, c1, c0, 0x00]` where `c0 c1 c2` are the three colour bytes of the
  LED as received by MSI's code. If the input is R, G, B (likely, unproven), the wire order is
  **B, G, R, pad**. **[V-local] for the reordering, [U] for what the input order is.**
- **Mystic Light itself does not offer per-LED control for this card.** Its card database
  describes the card as one lighting part ("RTX TRIO") with a list of firmware effects. The
  different colours the user sees on the strip, logo and underside come from the firmware's own
  animated effects. The per-LED path above exists in LEDKeeper2 only as the back end of MSI's
  "allow SignalRGB control" bridge. **[V-local], high confidence.**
- **Flash:** `0x3F` is the save register (MSI writes `0x3F` ← `0x00` one second after every
  effect change on this chip family; OpenRGB writes `0x01`). Avoid it entirely. `0x2E` is
  written by MSI on every effect change and every bridge start/stop, which suggests a volatile
  runtime flag **[U]**.
- **LED count and order:** not in the MSI data. MSI sends 34 slots whatever the card; which
  slot is which physical LED is unknown. Needs the hardware test below.

## Sources

All installed by MSI Center 2.0.75 / Mystic Light 3.0.37.2 in
`C:\Program Files (x86)\MSI\MSI Center\Mystic Light\`. Read only; no MSI code or data is
copied into this repository, only facts restated in our own words.

| File | Identity | Read with |
|---|---|---|
| `LEDKeeper2.exe` (.NET Framework 4.8, x86) | SHA-256 `070ebae715414beb4dba5bf6face3348c5b359ac4ae74ca4013c14be651bd891`, MVID `92aeb22f-1964-4574-be43-c0f5e8fdbefb`, assembly 3.0.37.2 | REA `inspect_managed_artifact` / `inspect_managed_members`; a small CIL disassembler of our own (Python, metadata tables + opcode table, kept in the scratchpad) |
| `Lib\NDA.dll` (native, i386) | SHA-256 `f1e2e40121af5b16328ed9b823e57b2d3d3f3a033963b660ae5a05ca3fdc9490` | export table parsed in Python; MinGW `objdump -d` over the exported functions |
| `MLModule.dll` (.NET) | | same CIL disassembler |
| `Mystic Light Online Data.dat` | MSI's device database, AES-encrypted text | decrypted in memory to the scratchpad with the method `LEDKeeper2` uses (below); not kept in the repo |
| `Support.cfg` | 2021 overclocking table | text |
| OpenRGB `d4798c12`, `Controllers/MSIGPUController/MSIGPUv2Controller/*` (GPL-2.0-or-later) | | cross-check of register names only |

Metadata tokens below are build-local to the `LEDKeeper2.exe` above.

No C# decompiler was needed: the relevant methods are short enough to read as CIL. If a later
pass needs whole-method C#, ILSpy (`ilspycmd`) would be the tool; it was not installed.

## 1. The per-LED path in LEDKeeper2

### 1.1 Where it lives

| Method (token) | What it does |
|---|---|
| `MSI_LED.SignalRGB.Signal_VGA::Init_VGA` (`0x06001218`) | reads the LED controller's firmware info, decides "NV5000" or not, then writes the enter command |
| `MSI_LED.SignalRGB.Signal_VGA::Set_LED_VGA` (`0x06001219`) | builds one frame from a colour array and writes it as one I2C block |
| `MSI_LED.SignalRGB.Signal_VGA::Release_VGA` (`0x0600121a`) | sleeps 100 ms, writes the leave command |
| `MSI_LED.RGBControlClass::updateSupportedDevice` (`0x06000c55`) | calls `Init_VGA` when the device list contains `10DE` and the registry value `LED\SignalRGB_Switch` is `ON` |
| `MSI_LED.RGBControlClass::RGBControl` (`0x06000c6a`) | parses the frames SignalRGB sends to MSI (UDP, loopback port 61337, `MSI_LED.UdpListener::.ctor` `0x06000c85`) and hands 34 LEDs to `VGA_SetRGB` |
| `MSI_LED.RGBControlClass::VGA_SetRGB` (`0x06000c5f`) | takes the named mutex `Global\Access_RGB_VGA`, runs `Set_LED_VGA` on a task, logs `VGA RGB LEDs send timeout` / `send mutex` on failure |
| `MSI_LED.Class_NDA::NDA_SetI2C` (`0x060010ab`), `::NDA_SetI2C_Block` (`0x060010ac`) | P/Invoke into `Lib\NDA.dll`: `(int adapter, int devAddr, int reg, int value)` and `(int adapter, int devAddr, int reg, byte[] data, int size)` |

`Set_LED_VGA` is the only place in LEDKeeper2 that writes register `0x49`, and `Init_VGA` the
only place that writes `0x2E` ← `0x01` (searched over every method body).

### 1.2 Card generation check

`Init_VGA` calls `NDA_ITE_GetFWVerInfo(0, out ledType, out moduleType, out fwVer)` and treats
the card as **"NV5000"** when `moduleType == 0x5000` or `ledType == 0x81`. Everything else,
including RTX 30 cards, takes the "v1" branch. `Class_Fun_VGA::GetFWType` (`0x060010d4`) logs
the same values as `NDA VGA LED Chip ID: …, VGA Card: …, LED Type: MS…, FW. Version: …`.

In `NDA.dll` (read with objdump):

| Export | I2C transfer (address `0xD0` 8-bit = `0x68`) | Result |
|---|---|---|
| `NDA_ITE_GetFWVerInfo` | read 4 bytes after register `0xEC` | LED type = byte 0; module type = byte 1 << 8 \| byte 2; firmware version = byte 3 |
| `NDA_ITE_GetChipID` | read 2 bytes after register `0xED` | chip id = byte 0 << 8 \| byte 1 |
| `NDA_ITE_GetFWVer` | read 4 bytes after register `0xEC` | returns bytes 2 and 3 |

These are reads only and make a safe first probe (section 5). The card's values are not in
any log on this machine (LEDKeeper2's own log was not found), so whether the reference card
is "v1" is **[U]**, but it is an RTX 30 card and NV5000 is, by its name and by the device
database (section 2), the RTX 50 generation.

### 1.3 The v1 sequence (this card, if not NV5000)

Every write goes to adapter index 0, device `0xD0` (7-bit `0x68`).

| Step | Transfer | Source |
|---|---|---|
| enter | write byte: register `0x2E` ← `0x01` | `Init_VGA` IL_0054..IL_005e |
| frame | write block: register `0x49`, data `4 × N` bytes, for LED i: `[in[3i+2], in[3i+1], in[3i], 0x00]` | `Set_LED_VGA` IL_0081..IL_00ec |
| leave | write byte: register `0x2E` ← `0x00` | `Release_VGA` IL_0024..IL_002e |

N is the number of colour triplets passed in. `RGBControl` always builds 34 (`0x22`) triplets
for the card: it takes the LED count n of the incoming package and repeats the package's
colours 34 / n times, then the first 34 mod n (IL_0276..IL_036d and again at IL_0940). So MSI
streams 34 slots, 136 bytes, per frame.

OpenRGB's MSI GPU v2 controller already names `0x2E` ("unknown", written `0x00` before every
mode change) but has no per-LED path; register `0x49` is not in OpenRGB.

### 1.4 The NV5000 sequence (for completeness; not this card)

| Step | Transfer |
|---|---|
| enter | register `0x06` ← `0x00` |
| frame | write block to register `0x04`: one byte `0x01`, then for LED i: `[i, in[3i], in[3i+1], in[3i+2], 0x01]` (5 bytes per LED, colour bytes not reordered) |
| leave | register `0x06` ← `0x01` |

That the two generations take the same input array but v1 reverses the three bytes while
NV5000 does not is the main hint that the v1 controller wants **B, G, R** **[U]**.

### 1.5 How NDA.dll performs the writes

`NDA_SetI2C`, `NDA_SetI2C_Block` and the reads are thin wrappers over internal helpers that
fill one `NV_I2C_INFO_V3` (version word `0x3002C`) and call NvAPI's extended I2C write/read:

| Field | Value |
|---|---|
| displayMask | 0 |
| bIsDDCPort | 0 |
| i2cDevAddress | `0xD0` (caller's argument) |
| register | 1 byte (`regAddrSize = 1`) |
| pbData / cbSize | the caller's buffer and size, **as is**: no SMBus count byte, no splitting |
| i2cSpeed | `0xFFFF` (use the kHz field) |
| i2cSpeedKhz | 6 (= 400 kHz in NvAPI's enum) |
| portId / bIsPortIdSet | 1 / 1 |

So the 136-byte frame is a single raw I2C write: `START 0xD0 0x49 <136 bytes> STOP`. A byte write
is the same with one data byte. The GPU handle comes from a table indexed by the adapter number
(MSI hard-codes 0 on this path).

## 2. How MSI decides what a card supports

There is no `IsPerLEDSupport` for graphics cards. Those strings (`IsPerLEDSupport`,
`MaxPerLedNum`, `FeatureData_PerLED`, `PerLedDataLength`, `Set_PerLed_*`,
`Set_SwitchToPerLEDMode`) belong to the motherboard classes (`MSI_7B10Led`, `MSI_7D26Led`,
`MSI_EAC8Led`, `MSI_7ZW0Led`, `DeviceData.MB.NuvotonMCU_*`). The graphics card's per-LED path
is not gated by any feature flag: it is used for any NVIDIA card while the SignalRGB bridge is
on.

What Mystic Light shows for a card comes from `Mystic Light Online Data.dat`, section
`[SyncData]`, one `V01|…` row per PCI id. `LEDKeeper2` reads it in
`Class_ParseCfg::ParseCfgFile` (`0x06000f35`) through `C_Encrypt::DecryptBase64` (`0x06000a31`):
strip the 7-character marker, Base64-decode, AES-CBC with key = SHA-256 and IV = MD5 of a short
constant built in `ParseCfgFile`. (The constant is not reproduced here.) The rows for subsystem
`3903` (device ids `2484`, `2486`, `2487`, `2488`, `2489`, `2503`, `2504`, i.e. the RTX 3060
Ti/3070/3070 Ti/3060 Trio family) are identical:

- one part, named "RTX TRIO";
- chip code `0x4E` (`'N'`);
- effect ids `13 43 4B 60 61 45 63 64 62 15 1B 1C 1E 01`, default colour red;
- no per-LED or LED-count field.

Chip codes seen in the same table: `0x48` (RTX 20 Duke), `0x49`/`0x4A`/`0x4B` (RTX 20 Gaming/
Trio variants), `0x4D` (RTX 3090 Trio), `0x4E` (this family), `0x50` (RTX 40 cards, matching
OpenRGB's "MSI GPU v2 / ITE9"), `0x51` (RTX 50 cards). `CControl::TimerControlVGA`
(`0x06000de5`) switches on this code for every effect.

For chip `0x4E`, in the effects inspected (ids `01` and `1B`), `TimerControlVGA` sends the classic one-colour registers OpenRGB knows (mode
`0x22`, colour `0x30..0x32`, brightness `0x36`, speed `0x38`), always preceded by `0x2E` ←
`0x00` and `0x22` ← `0x1C`, and followed one second later by `0x3F` ← `0x00` (IL_1a9ef..IL_1aa58).
Chip `0x48` has an extra "MultiColors" effect (12 colours, one 96-byte block to register `0x2D`,
8 bytes per entry); chips `0x50`/`0x51` write colours as 3-byte blocks to `0x27..0x29`. None of
these is a per-LED stream.

`Support.cfg` lists `2504/3903` too, but its columns are fan/clock settings, not lighting.

## 3. Registers and flash

| Register | Role (this family) | Persists? | Lightloom |
|---|---|---|---|
| `0x2E` | per-LED stream enable: `0x01` on, `0x00` off | **[U]** probably volatile: MSI writes it on every effect change | write `0x01` on entering direct mode, **always** `0x00` on leaving and on shutdown |
| `0x49` | per-LED frame, 4 bytes per LED | **[U]** volatile (MSI streams it per frame, no save after) | per frame |
| `0x3F` | save to flash (MSI writes `0x00` after each effect change, OpenRGB `0x01`) | **yes: avoid** | never on the per-LED path |
| `0xEC` (4 bytes), `0xED` (2 bytes) | firmware info / chip id, read only | n/a | read once to identify the generation |
| `0x22`, `0x30..0x32`, `0x36`, `0x38`, `0x26` | one-colour path (`docs/hardware/msi-gpu.md`) | volatile until `0x3F` | unchanged |
| `0x06`, `0x04` | NV5000 stream enable / frame | | only if the card reports NV5000 |

Bridge frames are not followed by any commit; the controller is expected to show a frame as
soon as the block is written **[U]**.

## 4. Open questions

1. Does this card's firmware honour `0x2E`/`0x49`? MSI sends it to every non-NV5000 NVIDIA card,
   but an RTX 30 controller might ignore it.
2. Colour order inside a record (B, G, R is the best guess).
3. How many of the 34 slots are wired on this card, and which slot is which LED (strip of 6,
   logo, underside). Do unused slots matter?
4. Does the firmware accept shorter frames (fewer than 34 records), or does it need all 136
   bytes? This decides whether Lightloom's 32-byte NvAPI limit is a blocker.
5. What the card shows after `0x2E` ← `0x00`: its stored effect, or the last frame?
6. Is `0x2E` volatile (lost on power cycle)? If a crash leaves it at `0x01`, does a reboot
   clear it?
7. Values of `0xEC`/`0xED` on this card (proves the v1 branch).
8. Does anything time out direct mode if frames stop (MSI streams continuously)?

## 5. Plan to confirm on hardware

### 5.1 Read-only capture first (the user's call: it needs elevation)

Goal: watch the exact I2C traffic MSI produces, without Lightloom writing anything.

1. In MSI Center, turn on the SignalRGB integration (the setting that writes
   `LED\SignalRGB_Switch` = `ON`), start SignalRGB with a simple multi-colour effect, and
   let MSI drive the card. This exercises `Init_VGA` / `Set_LED_VGA` / `Release_VGA`.
2. Hook, in the `LEDKeeper2.exe` process (32-bit), the two `NDA.dll` exports `NDA_SetI2C` and
   `NDA_SetI2C_Block` (both `__stdcall`, arguments as in section 1.1), or one level lower
   NvAPI's I2C write/read through `nvapi_QueryInterface`. Log adapter, address, register,
   length and data. Frida (`frida -p <pid>`) from an elevated shell is enough; LEDKeeper2 runs
   elevated, so the hook must too. Hook only, do not modify arguments.
3. Also hook `NDA_ITE_GetFWVerInfo` / `NDA_ITE_GetChipID` (their out-parameters) to learn the
   firmware info without reading the card ourselves.
4. Record: the enter write, ten or so frames with a known pattern (for example SignalRGB's
   solid red, then solid blue: if red arrives as `00 00 FF 00`, the order is B, G, R), stop
   SignalRGB (leave write), then change an effect in Mystic Light and check the `0x2E` ←
   `0x00`, `0x22` ← `0x1C`, …, `0x3F` ← `0x00` sequence.
5. Optional, no elevation needed: capture loopback UDP port 61337 to see what SignalRGB sends
   (package type, LED count), which explains the 34-slot tiling.

Expected result: confirms section 1.3 byte for byte, answers questions 2 and 7, and shows
whether the card actually changes per LED (question 1).

### 5.2 Safe test writes from Lightloom (after 5.1, user-triggered)

Pre-conditions: MSI Center's lighting service not driving the card (Mystic Light and the
SignalRGB bridge both off), Lightloom's one-colour plugin not running on the card.

Host work needed first **[D]** (done, §5.4): on NvAPI buses Lightloom's `WriteBlock` adds an SMBus count
byte and both block writes stop at 32 bytes (`crates/lightloom-nvapi/src/info.rs`,
`SMBUS_BLOCK_MAX`). The `0x49` frame needs a **raw I2C write** (register byte then data, no
count byte) of up to 136 data bytes. `WriteNoReg` with `[0x49, data…]` is the same transfer on
the wire, so the smallest change is a larger length limit for that op on NvAPI (MSI's NDA
passes 136 bytes to NvAPI in one call, so NvAPI accepts it).

Sequence, one step at a time, checked by eye:

1. Read `0xEC` (4 bytes) and `0xED` (2 bytes). Stop if module type = `0x5000` or LED type =
   `0x81` (wrong branch). Read `0x22` and `0x2E` (single bytes) and keep them.
2. `0x2E` ← `0x01`.
3. One frame to `0x49`: 34 records, record 0 = `FF 00 00 00`, the rest zero. Note which LED
   lights and in which colour. Then `00 FF 00 00`, `00 00 FF 00` for the colour order.
4. Walk record k = 1 … 33 with white `FF FF FF 00`, one frame each, to map slots to LEDs.
5. Try a short frame (for example 4 records, 16 bytes) to answer question 4.
6. `0x2E` ← `0x00` and confirm the card returns to its own effect. Then power-cycle the PC
   once and confirm nothing changed (question 6).

Never write `0x3F`, `0x22`, or any register not listed above during these tests. If the card
does not react to step 3, leave with step 6 and record the result; the one-colour path stays.

### 5.3 If it works

A per-LED mode in `plugins/msi-gpu/`: identify generation with the reads, enter with `0x2E` ←
`0x01` in `Initialize()`, one raw 136-byte write per changed frame in `Render()`, and `0x2E` ←
`0x00` (read back, retried once, as the current plugin does) in `Shutdown()`. The card stays
out of `tested` until 5.2 has been done on the reference card.

Implemented, unverified **[D]**: the parameter "Per-LED (experimental)" (`perLed`, default off)
in `plugins/msi-gpu/plugin.js` does the above with 34 slots as a 34 x 1 strip and `[B, G, R, 0]`
records; Shutdown leaves with `0x2E` ← `0x00` before the usual restore. Details in
`docs/hardware/msi-gpu.md`.

### 5.4 The probe

The host now takes register-less writes of up to 140 bytes on NvAPI buses (elsewhere 32): past
32 bytes NvAPI gets the first byte as the register (`regAddrSize = 1`) and the rest as data,
the call shape of §1.5; `SmbusDeniedRegisters()` still sees the first byte as the register,
so `[0x3F, …]` is refused at any length.

`lightloom probe msi-gpu-perled` (`crates/lightloom-cli/src/probe.rs`) is the §5.2 sequence,
with the user watching:

1. stop lightloomd, SignalRGB and MSI's lighting service (the probe refuses to run beside
   them); optionally rehearse with `--dry-run --auto 0` (no NvAPI);
2. run `lightloom probe msi-gpu-perled` (only `10DE:2504 1462:3903` is accepted);
3. note the printed `0xEC`/`0xED` values (question 7); the probe stops there, writing nothing,
   for an NV5000 controller or a failed read;
4. after `0x2E` ← `1`, for each slot 0..33 (red, `--brightness`, default `0x40`, in the
   `--order` channel order, default `bgr`) note which LED lit and its colour, then press Enter
   (`--auto <ms>` steps on its own; `q` stops) (questions 1 and 3);
5. all slots red for 3 s, then blue for 3 s: if red looks blue, the order is `rgb`
   (question 2);
6. a short frame of 6 slots (24 bytes, green): its result code is printed; slots 6.. staying
   blue means short frames are accepted (question 4);
7. the probe always ends with `0x2E` ← `0` (also on errors, Ctrl-C, console close, a panic);
   note what the card shows then (question 5); later, power-cycle and check (question 6).

It logs every transfer with its bytes and cannot write anything but `0x2E` ← `0`/`1` and
`0x49` frames of at most 34 records.
