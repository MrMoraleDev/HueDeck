<!-- SPDX-License-Identifier: Apache-2.0 OR MIT -->
# ADR-0007: Lightloom never ships a custom kernel driver

- Status: Accepted (2026-10-17); permanent
- Deciders: project owner, architecture panel (synthesis D7, D14, D17, section 9)
- Related: [ADR-0004](0004-localsystem-iobroker-pawnio.md), [SECURITY.md](../../SECURITY.md)

## Context

The reference application installs `SignalRgbDriver.sys`, a KMDF driver exposing
port I/O and PCI configuration access. It restricts callers by process image name
and by verifying an RSA/SHA-256 signature over the calling executable, registers
process callbacks, and if absent the application silently skips every SMBus, GPU
I2C and SuperIO device. Historically the RGB ecosystem relied on generic port-I/O
drivers (WinRing0, inpoutx64) that are now on vulnerable-driver blocklists.

A driver of our own would require EV code signing and Microsoft attestation (cost
and a legal entity), ongoing maintenance against HVCI / vulnerable-driver policy,
and would give any bug in the driver kernel-level impact on every user, including
the majority who only own HID devices.

## Decision

- Lightloom **never authors, signs or ships a `.sys` or any other kernel-mode
  component**, on any platform. This applies to the core repository, the installer
  and any first-party package.
- Hardware access is user-mode only:
  - HID through `hid.dll` (hidapi `windows-native`) / `hidraw`;
  - raw USB through WinUSB (`nusb`), bound to user-approved VID:PIDs by
    `lightloom-setup` / driverless on Linux;
  - serial through the OS serial stack;
  - LampArray devices through WinRT;
  - SMBus / SuperIO / MSR on Windows only through the optional broker talking to
    **PawnIO** (third-party, signed, open source, sandboxed Pawn modules, hashes
    pinned, "Unrestricted" builds refused), see ADR-0004;
  - SMBus / fans on Linux through `/dev/i2c-*` and hwmon sysfs with udev rules.
- `deny.toml` bans crates that wrap known-vulnerable port-I/O drivers (`winring0`);
  pull requests adding any driver binary, driver build or raw port-I/O path outside
  the broker are rejected.
- Features that cannot be delivered without kernel code beyond what PawnIO modules
  provide are **not delivered** (documented as unsupported) rather than solved with
  a driver.

## Consequences

Positive:

- No signing infrastructure, no attestation, no blocklist risk, no kernel-mode
  attack surface attributable to the project.
- Users of HID/USB/serial/network devices install nothing privileged at all.
- The security model stays explainable in one paragraph (see `SECURITY.md`).

Negative / accepted costs:

- SMBus/SuperIO features depend on PawnIO's module coverage and on the user opting
  in; some chips (e.g. Nuvoton SMBus on NCT6687D) may stay unsupported on Windows
  until a module exists.
- A one-time UAC prompt for the broker and PawnIO when the user opts in.
- Slightly higher per-transaction latency on SMBus (IPC hop to the broker); this is
  irrelevant at RGB update rates.

## Alternatives considered

- **Own KMDF driver with image-name and signature checks** (reference approach):
  rejected for cost, risk and the fact that the check binds the driver to one
  executable, which is exactly what an open project cannot promise.
- **WinRing0 / inpoutx64 / other generic port-I/O drivers**: vulnerable by design
  (arbitrary port and MSR access from user mode), blocklisted.
- **Reusing the reference app's driver**: locked to its own signed binaries, not
  redistributable.
- **Linux-only SMBus support**: simpler but abandons Windows users; the broker
  design (ADR-0004) covers the gap without kernel code of our own.
