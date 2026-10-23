<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# Security policy

## Reporting a vulnerability

Please do **not** open a public issue for security problems.

- Email: `security@lightloom.invalid` (placeholder; will be replaced by a real
  address and a PGP key before the first public release)
- Alternatively use GitHub's private vulnerability reporting on the repository once
  it is enabled.

Include the Lightloom version or commit, OS, the component (daemon, renderer, broker,
CLI, dashboard, a first-party plugin), reproduction steps and the impact you
believe it has. You will get an acknowledgement within 7 days and a coordinated
disclosure plan; we aim to fix confirmed issues within 90 days and will credit you
unless you prefer otherwise.

## Supported versions

Pre-alpha: only the `main` branch is supported. Once releases exist, the latest
minor release receives security fixes.

## Stance: no kernel driver, least privilege

- Lightloom **never ships a kernel driver** ([ADR-0007](docs/adr/0007-no-custom-kernel-driver.md)).
  The daemon, renderer, CLI and dashboard always run as the logged-in user. HID,
  USB (WinUSB), serial, network and LampArray devices work with nothing privileged
  installed.
- SMBus / GPU-I2C / SuperIO on Windows require kernel access. Lightloom obtains it
  **only** through the optional `lightloom-iobroker` Windows service and the
  third-party, signed, open-source **PawnIO** driver, installed once on explicit
  opt-in ([ADR-0004](docs/adr/0004-localsystem-iobroker-pawnio.md)). PawnIO
  module binaries are SHA-256 pinned; "Unrestricted" PawnIO builds are refused.
  Linux uses `/dev/i2c-*` and hwmon with udev rules; no broker.
- Generic port-I/O drivers (WinRing0, inpoutx64, ...) are banned by `deny.toml`.

## Threat model (summary)

Full text, with what is in place today and what is still planned (the Chromium sandbox,
manifest permission enforcement, plugin source pinning, signing):
[docs/security.md](docs/security.md). The design background is in
[docs/architecture-synthesis.md](docs/architecture-synthesis.md) section 9.

| Asset / boundary | Threat | Control |
|---|---|---|
| **Device plugins** (JavaScript, imported from the user's own copies or community sources; treated as untrusted-ish) | Arbitrary code reaching the filesystem, the network, other devices, or hanging the daemon | QuickJS has no fs/net primitives unless injected; memory cap 64 MB per runtime; interrupt watchdog (250 ms of script time per `Render()`, seconds-scale for `Initialize()`; three strikes -> fault + backoff); HID plugins receive only their matched device handles; SMBus plugins only the scanned bus and a `Scan()`-derived write allowlist; sockets and HTTP only through host modules with per-plugin consent (`@SignalRGB/permissions`); UDP/TCP binds are opened only while a network plugin is active; `XmlHttp` / `XMLHttpRequest` additionally pass a per-plugin host allowlist (RFC1918/loopback denied unless declared) |
| **Effects** (HTML in Chromium) | Malicious page escaping the renderer, exfiltrating data, reaching the daemon | Separate process; Chromium sandbox on; URL request filter allows only the effect's own directory and the `lightloom://` prelude; no clipboard; the only daemon-facing surface is the `engine` object (read-only data) and canvas frames over shared memory |
| **Privileged broker** (`lightloom-iobroker`, LocalSystem, optional) | Local privilege escalation via the broker; bus corruption; SPD/firmware damage | Named pipe DACL limited to the interactive user's SID + per-boot token; closed operation enum, every argument range-checked; single-threaded; every SMBus transaction under `Global\Access_SMBUS.HTP.Method`; address allowlists; SPD EEPROM (0x50-0x57) and write-protect (0x30-0x37) writes denied by default; 2000 ops/s rate limit; dry-run; per-plugin kill switch; audit log; fuzzed parser; < 2k LOC |
| **Local HTTP/WS API** (`127.0.0.1:53000` native by default, `:16038`/`:16034` compat) | Other local users or web pages driving the daemon (CSRF, DNS rebinding) | Loopback only; mutations require a bearer token from a file ACL'd to the user; compat ports refuse to bind when another application owns them; mutating compat calls need the token by default; CORS restricted |
| **Plugin / effect sources** (community repositories, first-party packs) | Supply-chain tampering, silent updates | Hash-pinned; updated only on user action with a diff summary; per-repo license acknowledgement; project ed25519-signed index for first-party packs; imported WhirlwindFX files marked `unsigned` |
| **Build and release** | Compromised dependencies, unreproducible binaries | `cargo-deny` (licenses, advisories, bans, registry allowlist), `cargo-auditable`, CycloneDX SBOM, reproducible `cargo-dist` builds, Authenticode via SignPath OSS (planned), Dependabot |
| **Other RGB software** | Device contention, bus races | Conflicting processes are *reported* with a "close" button and never killed; SMBus coexistence through the standard global mutexes; HID handles opened only after conflicts are resolved |
| **Privacy** | Telemetry, cloud accounts | None. No accounts, no analytics, no crash upload; diagnostics export is manual and local |

Out of scope: physical access to the machine, a compromised administrator account,
vulnerabilities in third-party drivers (report those to PawnIO) or in device
firmware.

## Hardening checklist for contributors

- Never add a code path that performs port I/O, MSR or PCI config access outside
  `lightloom-iobroker`.
- New broker ops require: enum entry in `lightloom-protocol`, range checks, a fuzz
  corpus entry, an audit-log line and an ADR update.
- New host modules exposed to plugins must be gated behind `addFeature` /
  `permissions` and documented in `docs/host-api/`.
- Do not widen the renderer's URL filter or disable the Chromium sandbox, even for
  debugging builds that could ship.
