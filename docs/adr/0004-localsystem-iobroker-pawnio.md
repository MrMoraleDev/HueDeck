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
