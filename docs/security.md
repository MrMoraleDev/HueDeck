<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# HueDeck security: threat model and current state

This is the threat model of HueDeck as it is built on `develop` (2026-10-23), with each
control marked as **in place** or **planned**. `SECURITY.md` is the short policy (how to report
a problem, the no-kernel-driver stance); this file is the detail behind it. Where the two
differ, this file describes what the code does today.

## What HueDeck is, seen from an attacker

| Process | Runs as | Talks to |
|---|---|---|
| `huedeckd` (daemon, dashboard server, plugin host) | the logged-in user | HID devices, GPU I2C (NvAPI), the I/O broker pipe, the renderer (shared memory + stdin), loopback HTTP |
| `huedeck` (CLI) | the logged-in user | the same devices when it runs headless; the daemon's API otherwise |
| `huedeck-renderer` (CEF, one per effect) | the logged-in user | the effect's files; the daemon (shared-memory frames, a stdin control pipe) |
| `huedeck-iobroker` (optional service) | **LocalSystem** | the PawnIO driver; one named pipe |
| `huedeck-setup` (optional, once) | Administrator (one UAC prompt) | the service manager, `%ProgramFiles%\HueDeck` |
| PawnIO (third-party signed driver, optional) | kernel | the broker only |

The assets: the user's machine (code execution, privilege), the hardware (RAM SPD and other
EEPROMs, firmware, fans and voltage controllers on the same buses), and the user's data
(files, network, input).

## 1. Local API on the loopback interface

**Surface.** `huedeckd` serves the native REST + WebSocket API and the dashboard on
`127.0.0.1:53000` (configurable; `docs/api/openapi.yaml`), and, when `[api] compat = true`
(the default), the SignalRGB-compatible REST API on 16038 and its canvas-event listener on
16034. The OpenRGB SDK port 6742 is reserved but not served yet.

**Threats.** Another program or another local user driving the lights, changing settings or
shutting the daemon down; a web page in the user's browser doing the same (CSRF, DNS rebinding).

**Controls in place** (`crates/huedeck-api/src/server.rs` `guard`):

- Every listener binds `127.0.0.1` only; no setting widens it.
- Every request must carry a loopback `Host` header (`127.0.0.1:<port>`, `localhost:<port>`,
  `[::1]:<port>`), else `403 forbidden_host`. A DNS-rebinding page sends its own host name, so
  it is refused.
- An `Origin` header, when present, must be the same loopback origin, else
  `403 forbidden_origin`. Browsers always send `Origin` on cross-origin requests, so other web
  pages cannot use the API even through `fetch` with `no-cors`.
- `POST`, `PUT`, `PATCH` and `DELETE` on the native API need `Authorization: Bearer <token>`.
  The token is 64 hex digits from two random UUIDs, created on first start in the `api-token`
  file of HueDeck's data directory (`%LOCALAPPDATA%\HueDeck` on Windows, mode 0600 on Unix);
  the comparison is constant-time. Reads (`GET`) need no token: the state is not secret
  enough to protect from other local processes of the same user.
- The dashboard page gets the token embedded when served, under a strict
  `Content-Security-Policy` (`default-src 'self'; frame-ancestors 'none'`) and `no-store`.
- The compat REST API (16038) requires the same token for mutations unless the user sets
  `compat_open_mutations = true` in `config.toml` (SignalRGB clients send no token).
- The compat listeners start only once no other program (typically SignalRGB) listens on
  their port, and retry; HueDeck never takes over a port another program holds.
- Changing the native port (`PATCH /v1/settings {"port"}`) needs the token, accepts only
  1024-65535 outside HueDeck's own reserved ports (`400 invalid_port`), and binds the new
  port before anything changes (`409 port_in_use`).

**Known gaps.**

- The canvas-event listener (16034, `POST /canvas/event`) accepts events **without** the
  token by default (`canvas_events_need_token = false`), as SignalRGB's does; an event only
  calls the current effect's `onCanvasApiEvent`. Set it to `true` when no third-party sender
  is needed.
- On Windows the token file relies on the ACL of the user's profile directory; HueDeck does not
  set its own ACL on it. Other processes of the same user can read it, by design: they can
  already act as the user.
- Any process of the same user that reads the token can do what the dashboard does.

## 2. Plugins (device drivers in JavaScript)

**Surface.** Plugins are ES modules run by QuickJS-NG (rquickjs), one runtime per plugin on
its own OS thread. They come from: HueDeck's own `plugins/` (first-party, GPL-3.0-or-later,
reviewed), the user's plugin directory, `[plugins] dirs` in `config.toml`, and, only when the
user turns the SignalRGB setting on, a local SignalRGB installation read in place. Later
directories win for the same VID:PID.

**Trust model.** A plugin is code the user chose to install, but HueDeck limits what it can
reach so a bad or buggy plugin cannot take over the machine or damage hardware outside its own
device.

**Controls in place.**

- QuickJS has no file system, process, network or timer primitives; the host injects only the
  documented API (`docs/host-api/`, `sdk/huedeck.d.ts`). Modules resolve only to the host's
  `@SignalRGB/*` / `@huedeck/*` shims and to files inside the plugin's own directory
  (`crates/huedeck-plugin-host/src/resolve.rs`). An unknown host member or module throws
  `HostApiMissing` in a device session.
- 64 MiB heap per runtime; an interrupt deadline per call (JavaScript time, not time spent in
  `device.pause` or I/O): 250 ms per `Render()`, 1 s for `Validate()`, 10 s for loading and
  `Initialize()`, 5 s for `Shutdown()`, with wall-clock caps. Consecutive `Render()` timeouts fault the
  session, which is stopped and reported.
- HID: a plugin gets only the collections of the devices that match its `VendorId()` /
  `ProductId()` (or `HidUsages()`) and that its `Validate()` accepted. `[devices] ignore` in
  `config.toml` keeps devices away from every plugin.
- Network: `XMLHttpRequest` / `XmlHttp` are stubs whose requests never leave the process;
  `device.addFeature("udp" | "dtls")` and the `@SignalRGB/udp`, `tcp`, `serial` and `lcd`
  modules throw `HostApiMissing`. Plugins have no network access today.
- SMBus (section 3): an SMBus plugin runs only if it declares `SmbusAddresses()`; it reaches
  only those addresses during `Scan()`, and only the address its scan returned afterwards;
  `SmbusDeniedRegisters()` and the host's own deny ranges hold in every phase.
- `--dry-run` (CLI and daemon setting) records SMBus writes without sending them.
- First-party plugins are checked in CI by `plugins/_testkit/manifests.test.mjs`, which runs
  `sdk/lint` (manifest schema, exports against the manifest, no DOM/Node/timer globals).

**Known gaps (planned).**

- **Manifest permissions are not enforced at run time yet.** `manifest.json` declares
  `permissions` and `devices`, and the lint checks them, but the host does not yet refuse a
  call because the permission is missing. Today the limits above come from what the host
  implements, not from the manifest. Enforcement lands with the network and serial work.
- Plugins from the SignalRGB installation (opt-in) are third-party code under their own
  licence; HueDeck runs their HID side with the same limits, and refuses their SMBus side
  (they have no `SmbusAddresses()`).
- There is no signature or hash pinning of plugin files yet (section 7).

## 3. The privileged I/O broker and PawnIO

**Surface.** RAM lighting needs the chipset SMBus, which only kernel code can reach. HueDeck
never ships a kernel driver (ADR-0007). On the user's explicit opt-in, `huedeck-setup --smbus`
(elevated, once) registers `huedeck-iobroker`, a LocalSystem service that talks to the
third-party signed **PawnIO** driver, which the user installs themselves. GPU I2C (NvAPI) does
not use the broker: it runs in the user's own process.

**Threats.** Local privilege escalation through the broker; a process of the user (or of
another user) writing to SPD EEPROMs, clock generators, VRM or EC parts on the SMBus; bricked
DIMMs.

**Controls in place** (`crates/huedeck-iobroker`, ADR-0004):

- Nothing in HueDeck downloads or installs PawnIO; `huedeck-setup` refuses to proceed unless
  the official PawnIO edition 2.2.0 or later is installed, and never installs it.
- The broker loads only the PawnIO modules whose size and SHA-256 are compiled into it
  (`pins.rs`, the signed `PawnIO.Modules` 0.2.11 release).
- One named pipe, `\\.\pipe\huedeck-iobroker`; remote clients refused; the DACL admits
  SYSTEM and the one interactive user recorded by `huedeck-setup` (read/write, not
  create-instance).
- A closed set of postcard-encoded operations with a size cap; every argument range-checked
  per bus kind before the bus mutex is taken.
- Static policy no client can change: writes and Quick commands to the DIMMs' own parts are
  refused (SPD and EEPROM area 0x50-0x5F, write protect / page select 0x30-0x37, DDR5 PMIC
  0x48-0x4F); writes anywhere else only to the first-party chipset plugins' addresses
  (0x20-0x23, 0x27, 0x39-0x3C); only chipset buses (no GPU, no SuperIO); `SpdRead` only at
  0x50-0x57, its page select done inside the broker and always put back to page 0.
- At most 2000 bus transfers per second; every transfer under the system-wide
  `Global\Access_SMBUS.HTP.Method` mutex, held at most 250 ms for a lease; one worker thread.
- An audit line for every write; dry-run refuses writes visibly.
- Unsafe code only in the three FFI files, each block with a `SAFETY:` note.

The user-side client adds its own layers before a request reaches the pipe: the per-plugin,
per-scan address allowlist, `SmbusDeniedRegisters()`, the DIMM deny ranges on every bus, and
the conflict check (no SMBus while SignalRGB, OpenRGB or a vendor lighting service runs).

**Known gaps (planned).**

- No per-boot token proving the client is HueDeck: the DACL is the gate, so any process of the
  recorded user can send requests; the static policy above bounds what they can do.
- No fuzz target for the broker's parser yet (`feature/broker-fuzz`).
- Only AMD PIIX4 buses; Intel i801 and SuperIO are not served.

## 4. Effects and the renderer

**Surface.** Effects are HTML pages ("lightscripts"), first-party (CC0), the user's own, or
from a SignalRGB installation (opt-in). Each runs in `huedeck-renderer`, a windowless CEF
(Chromium) process, which paints the canvas into shared memory.

**Controls in place** (`crates/huedeck-renderer`):

- A separate process per effect; a crash or hang takes down only the renderer, which the
  daemon supervises and restarts.
- A request filter in the browser process cancels every load and navigation except files in
  the effect's own directory, `data:`, `blob:`, `about:blank` and `about:srcdoc`; popups are
  refused. `http(s)` and `ws(s)` pass only with `--allow-network`, which the daemon does not
  use; host-name resolution is blocked too (`host-resolver-rules=MAP * ~NOTFOUND`).
- The only daemon-facing input is the injected `window.engine` object (read-only data) and
  control values the daemon writes as JS literals of validated identifiers
  (`inject.rs` refuses reserved words and navigating globals).
- The Chromium cache and profile live in a directory created and wiped per run.

**Known gap.** **The Chromium sandbox is off.** CEF 138+ on Windows sandboxes only through
`bootstrap.exe` / `bootstrapc.exe`, which must load a signed client DLL; a sandboxed build
was measured to work (`crates/huedeck-renderer/README.md`, "Chromium sandbox"), and it is
turned on together with release signing (WP-SIGN). Until then a renderer exploit runs with
the user's rights, bounded only by the request filter.

## 5. Other RGB software and the hardware

- Conflicting programs (SignalRGB, OpenRGB, vendor services) are detected and reported, never
  killed. HID devices they hold are left alone unless the user overrides it; SMBus never runs
  while one of them runs (no override).
- One HueDeck process drives SMBus at a time (`Global\HueDeck.SmbusOwner`).
- Plugins send lighting commands only; first-party plugins never write device flash or
  EEPROM (registers that would are listed in `SmbusDeniedRegisters()` and refused by the host),
  and never touch fan, DPI or audio settings.

## 6. Privacy

No telemetry, no accounts, no crash upload, no update checks. HueDeck makes no network
connection of its own; the only listeners are the loopback ones above.

## 7. Updates, downloads and plugin sources

- HueDeck does not update itself and downloads nothing at run time: no plugin, effect,
  component or driver is fetched automatically. Plugins and effects arrive only as files the
  user puts in place (or the SignalRGB installation the user opted into, read in place).
- Release artefacts are built in CI with `cargo-auditable` and a CycloneDX SBOM;
  `cargo-deny` checks licences, advisories and banned crates (generic port-I/O drivers such
  as WinRing0 are banned). Authenticode signing (SignPath) is **planned** (WP-SIGN).
- **Planned** (WP-SOURCES, `schemas/plugin-source.schema.json`): plugin sources (git or HTTPS)
  that the user adds explicitly, with a licence acknowledgement per source, hash pinning of
  what was reviewed, and updates only on user action with a summary of what changed.

## Reporting

See `SECURITY.md`.
