<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# ADR-0004: Privileged bus access through an optional LocalSystem broker over PawnIO

- Status: Accepted (2026-10-17); implemented in M5
- Deciders: project owner, architecture panel (synthesis D7, D8, D17)
- Related: [ADR-0007](0007-no-custom-kernel-driver.md), [docs/host-api/smbus.md](../host-api/smbus.md), [docs/architecture-synthesis.md](../architecture-synthesis.md) sections 3, 5 and 9

## Context

RAM, motherboard and some GPU lighting is driven over SMBus/I2C (chipset PIIX4 /
I801 controllers, GPU I2C through NvAPI/ADLX) and SuperIO LPC (fan control, Nuvoton
and ITE chips). On Windows these need port I/O, PCI configuration access or
MSR reads, which are kernel-only. The reference app ships its own signed kernel
driver locked to its own executable; HueDeck will not (ADR-0007).

PawnIO is a signed, open kernel driver that executes small sandboxed "Pawn"
modules (`SmbusPIIX4`, `SmbusI801`, `LpcIO`, MSR, ...). Its device object is
administrator-only, so an unprivileged daemon cannot open it directly, and asking
for UAC on every session is unacceptable. Other tools (HWiNFO, OpenRGB, vendor
suites) share the same buses and coordinate through global named mutexes such as
`Global\Access_SMBUS.HTP.Method`.

HID, raw USB, serial and network devices need none of this and must keep working
with zero privileged components.

## Decision

- A separate binary `huedeck-iobroker` runs as a **Windows service under LocalSystem**.
  It is **optional**: installed once, with a single UAC prompt, by `huedeck-setup`
  only when the user opts into SMBus / GPU-I2C / SuperIO. Linux does not have a
  broker at all (`i2c-dev` + udev group, hwmon for fans).
- Transport: a named pipe whose DACL is restricted to the interactive user's SID,
  plus a per-boot token stored in a file ACL'd to that user. Messages are
  `postcard`-encoded with a **closed operation enum** (about 12 ops) defined in
  `huedeck-protocol`; every argument is range-checked (address 0x03-0x77, block
  length <= 32, known bus ids).
- The broker is **single-threaded** and wraps **every** SMBus transaction in
  `Global\Access_SMBUS.HTP.Method` (ISA/PCI mutexes for SuperIO), not only inside a
  plugin's `TransactionLock`, so plugins are serialized by construction and HWiNFO /
  OpenRGB coexist.
- Safety rails: write allowlists derived from each plugin's `Scan(bus)` answers;
  writes to SPD EEPROM addresses 0x50-0x57 and write-protect range 0x30-0x37 denied
  by default; 2000 ops/s rate limit; `--dry-run` mode; per-plugin kill switch; audit
  log; a fuzz target for the message parser; whole crate kept under ~2k LOC.
- PawnIO usage: client only. Module binaries are **SHA-256 pinned**
  (`tools/pawnio-hash-pin`); "Unrestricted" PawnIO builds are refused. Any Pawn
  module HueDeck needs beyond the official set is written under LGPL-2.1 and
  upstreamed to PawnIO.Modules: official PawnIO builds load only modules signed by
  the PawnIO maintainer, and the IOCTL exception does not cover the Pawn side
  (amended 2026-10-18, see below).
- GPU I2C uses `huedeck-nvapi` (dynamic `nvapi64.dll`, self-declared interface ids)
  inside the broker; ADLX later. Sensors (MSR, NvAPI thermal) are read only through
  the broker; an optional LibreHardwareMonitor HTTP JSON source gives temperatures
  with zero kernel involvement from us.

## Consequences

Positive:

- Users of HID/USB/serial/network devices never install anything privileged; the
  daemon, renderer and UI always run as the user.
- No kernel code is authored or signed by the project.
- Bus corruption from interleaved plugins is impossible by design; concurrency with
  other monitoring tools is handled with the same mutexes they already use.
- One UAC prompt at setup, none per session.

Negative / accepted costs:

- One more process and a Windows service to package, start and audit.
- A hard dependency on a third-party driver project (PawnIO) and on its module
  coverage; Nuvoton SMBus (NCT6687D) support is an open question for M5.
- A LocalSystem component is still high-value to an attacker; hence the tiny closed
  interface, the pipe DACL, the fuzzing and the audit log.
- Windows-only complexity; the Linux path is intentionally different and simpler.

## Alternatives considered

- **Own kernel driver** with image-name / signature checks like the reference app:
  rejected (ADR-0007).
- **WinRing0 / inpoutx64**: known-vulnerable generic port-I/O drivers; `winring0` is
  banned in `deny.toml`.
- **Per-session elevation (`runas`)**: UAC on every login; rejected by the judges.
- **Opening PawnIO from the unprivileged daemon**: impossible, its device is
  administrator-only.
- **LocalService / NetworkService account**: cannot open the PawnIO device either;
  LocalSystem is required, so the privileged surface is minimized instead.
- **Running the entire daemon elevated**: puts the plugin host, Chromium and the
  HTTP API in a privileged process.
- **Dropping SMBus support**: loses RAM, many motherboards and GPUs; too much of the
  ecosystem.

## Amendments

### 2026-10-18: findings of the M5 research brief

Source: [docs/research/m5-privileged-buses.md](../research/m5-privileged-buses.md)
(drafted, then independently fact-checked against primary sources). Decided now:

- **Pawn modules** are LGPL-2.1 and upstreamed to PawnIO.Modules (see the Decision
  section). HueDeck ships the signed module blobs of PawnIO.Modules 0.2.11
  (`SmbusPIIX4`, `SmbusI801`, `LpcIO`, plus `Echo` as a probe) as separate files, with
  SHA-256 pins compiled into the broker.
- **PawnIO itself** is required at version 2.2.0 or later, official edition, installed
  by its official installer on opt-in (never a `.sys` inside HueDeck's installer,
  ADR-0007) and never uninstalled by HueDeck, because other tools share it.
- **Device path** is `\\?\GLOBALROOT\Device\PawnIO`; the broker talks raw IOCTLs and
  does not load `PawnIOLib.dll` into the LocalSystem process.
- **Block transfers** are limited to 32 bytes per bus (the wire's `MAX_BLOCK_LEN` of 64
  is only the protocol ceiling), and the SPD write denial must depend on the bus kind.
- **Reference machine**: two PIIX4 buses (0x0B00 with the DIMMs, 0x0B20 auxiliary) and
  the RTX 3060 over NvAPI. The NCT6687D SuperIO (LPC config port 0x4E, base 0xA20)
  exposes no SMBus master that HueDeck can use.

Open, to settle in the M5 spike:

- **NvAPI GPU I2C in `huedeckd` instead of the broker.** Any user process can already
  call NvAPI (SignalRGB does so unelevated on the reference machine), so loading
  `nvapi64.dll` into a LocalSystem service only adds attack surface. Whether NvAPI I2C
  works from session 0 is untested. If the spike confirms the user-process path, the
  NvAPI part of this decision moves to the daemon with the same op validation.

### 2026-10-19: NvAPI GPU I2C runs in the user's process

Decided for M5: GPU I2C goes through NvAPI **inside `huedeck` / `huedeckd`**, the
unelevated user process, not through the broker (`crates/huedeck-nvapi`). Any user process
can already call NvAPI (SignalRGB does so unelevated on the reference machine), so the
broker would add attack surface without adding capability. The policy the broker would
enforce is enforced in the plugin host instead, for every bus alike
(`crates/huedeck-plugin-host/src/smbus.rs`): the plugin's scan window, the allowlist from
the scan, no DDC/display ports (port 1, `bIsDDCPort = 0` only), SPD write denial on
every bus, the plugin's denied registers, and nothing while SignalRGB, OpenRGB or a vendor
lighting service runs.

- `nvapi64.dll` is loaded from System32 only and every call runs on one worker thread
  (`huedeck-nvapi`), which owns the GPU handles.
- Superseded (see the review amendment below): the worker now takes the global SMBus
  mutex around every NvAPI transfer.
- The broker keeps the chipset SMBus (PawnIO) only; its protocol work (v2, `BusLock`
  leases) belongs to the RAM part of M5.

### 2026-10-19: the broker for RAM lighting (chipset SMBus)

Implemented in M5 (`crates/huedeck-iobroker`, `crates/huedeck-hal-windows/src/broker.rs`,
`crates/huedeck-setup`), proven against fakes only until the run on the reference machine:

- **Broker protocol v2**: `BusLock { bus, lease_ms }` / `BusUnlock { bus }` (a plugin's
  `TransactionLock`; lease 1..=250 ms, the broker lets go when it runs out or the client
  leaves), `BrokerError::BusContended`, PawnIO version / edition / code-integrity / sleep
  mode in `Health`, and the SPD denial and 32-byte block limit applied by bus kind.
- **PawnIO** through raw IOCTLs on `\\?\GLOBALROOT\Device\PawnIO`; `SmbusPIIX4` from
  PawnIO.Modules 0.2.11, SHA-256 pinned, on one handle per port (0x0B00, 0x0B20); ports
  2-4 of the primary controller's mux are never selected for HueDeck's own use, and a mux
  another program left there is put back after each batch. Only the AMD FCH is served for
  now (the reference board); `SmbusI801` joins the same way.
- **Edition check**: code-integrity TESTSIGN off and a copy of `Echo` with a flipped
  signature byte refused by the driver; 2.2.0 or later.
- **Mutex**: `Global\Access_SMBUS.HTP.Method`, created with an Everyone DACL (or opened),
  50 ms bounded waits per operation or lease; no mutex, no SMBus.
- **Pipe**: one reused instance, remote clients rejected, DACL SYSTEM plus the interactive
  user's SID recorded by `huedeck-setup` (read/write without `FILE_CREATE_PIPE_INSTANCE`).
  **Deferred**: the per-boot token of the Decision section; the DACL is the gate for M5.
- **Allowlists** from `Scan()` live in the client's plugin host (it knows which plugin
  asks); the broker keeps the static rules (ranges, SPD, chipset kinds only, rate limit,
  dry-run, audit log) and, since the review below, its own write allowlist.
- **Setup** never downloads or installs PawnIO: it refuses when PawnIO is missing or too
  old, copies the user-supplied pinned modules (with `COPYING` and sources) and the
  broker executable under `%ProgramFiles%\HueDeck\`, and registers `HueDeckIoBroker`
  (LocalSystem, automatic start, unrestricted service SID, restart on failure).
  `--uninstall` removes only what it added. Not yet set: the service's required
  privileges list.

### 2026-10-19: hardening after the M5 review

The broker is the privilege boundary and the pipe client is not authenticated yet, so the
rules that matter for safety no longer rely on the client:

- **Broker write allowlist**: writes and Quick commands go only to
  `BROKER_WRITE_ALLOWLIST` (`huedeck-protocol`), the union of the first-party chipset
  plugins' `SmbusAddresses()` windows: 0x20-0x23, 0x27, 0x39-0x3C (Crucial). Everything
  else is `AddressDenied` before the mutex; reads are not limited. A new chipset plugin adds
  its window there, after review. A per-connection list sent after `Scan()` could narrow it
  later, never widen it.
- **DIMM deny ranges**, on chipset and Nuvoton buses (Nuvoton masters reach the DIMMs on
  some boards): writes and Quick commands to 0x30-0x37 (SPD write-protect), 0x48-0x4F
  (DDR5 PMIC) and 0x50-0x5F (SPD, DDR5 RCD, 24RF08-class EEPROMs that a Quick write can
  corrupt). 0x50-0x57 is denied on every bus. A future Corsair DDR4 port (controllers at
  0x58-0x5F) needs an explicit, reviewed exception.
- **Lease limits**: a `BusLock` hold never runs past 250 ms from its start, renewals and
  other buses' leases included; the mutex is then released (a waiting program gets it)
  before the next lease. While a lease holds the mutex the broker reads a request only
  once the whole frame has arrived, so a client stalling mid-frame cannot keep it.
- **Edition check fails closed**: a missing or altered pinned `Echo` blob makes PawnIO
  unavailable instead of skipping the tamper probe.
- **Setup** reads Program Files from `HKLM` (not the environment), takes the client SID
  from its process token (no `whoami` from the search path), and checks the broker copy
  against the hash of the file it read. The remaining trust root is the folder setup runs
  from: someone who can replace the broker there can replace `huedeck-setup.exe` too, so
  only signed releases (an Authenticode check) close that; open until releases are signed.
- **GPU I2C and the global mutex** (decision): HueDeck's rule is that SMBus traffic runs
  under `Global\Access_SMBUS.HTP.Method`, and it now covers NvAPI too. The NvAPI worker
  creates or opens the mutex (Everyone DACL, as the broker) and holds it around every
  transfer (50 ms bounded wait, `NVAPI_DEVICE_BUSY` to the plugin when another program
  keeps it) and across a `TransactionLock` lease (at most 250 ms). Without the mutex no
  GPU is driven. OpenRGB and SignalRGB do not take it for NvAPI, so it orders HueDeck
  against the programs that follow the convention (and HueDeck's own processes) only;
  vendor tools are kept away by the process check below.
- **One driver at a time**: `huedeckd` and `huedeck --smbus` take `Global\HueDeck.SmbusOwner`
  before touching SMBus; the second one refuses. SignalRGB, OpenRGB and the vendor
  lighting services that drive the same controllers (MSI `LEDKeeper*` /
  `LightKeeperService*` / `Mystic_Light_Service`, ASUS `LightingService`, Gigabyte
  `RGBFusion`) are looked for before every scan and every second while SMBus devices run;
  one that starts stops the SMBus sessions (the daemon) or the run (the CLI).
- **NvAPI identity**: requests name the card by PCI bus and ids, resolved at every
  transfer; a card gone after a driver reset fails instead of reaching whatever GPU now
  sits at the old index. Requests whose caller timed out are dropped, never sent late.
