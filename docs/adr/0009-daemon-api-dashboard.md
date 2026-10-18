<!-- SPDX-License-Identifier: Apache-2.0 OR MIT -->
# ADR-0009: lightloomd's engine thread, the loopback API and the dashboard

- Status: Accepted (2026-10-18); implemented in M4
- Deciders: project owner
- Related: [ADR-0008](0008-persistence-sqlite.md), [docs/architecture.md](../architecture.md) §1, §3, §7

## Context

M4 turns the command-line pipeline (M1-M3) into a resident daemon with a
graphical front end. Several things must stay true at once:

- device sessions (one plugin thread each), the effect renderer and the database
  change together and must not race (a layout switch moves endpoints of several
  sessions; an import writes the database and restarts sessions);
- the dashboard and other local tools need a read model that is always available,
  even while the engine is busy stopping a slow plugin;
- the API listens on the loopback interface, which every web page in the user's
  browser can also reach (cross-site requests, DNS rebinding);
- SignalRGB, when installed and running, owns the ports of its own local API
  (16038, 16034); Lightloom must not take its clients away;
- the dashboard should not need Node.js at run time or a webview runtime.

## Decision

- **One engine thread** (`lightloom-daemon::engine`) owns the `DeviceManager`, the
  canvas renderer and the SQLite connection. Every state change arrives as a
  `Request` (`lightloom_api::model`) with a reply channel; the engine applies it,
  publishes a new `Snapshot` and only then replies (a client reads its own writes).
  Plugin sessions get runtime changes as controls (parameters, brightness,
  placement of the device and of each subdevice or component instance); changes of
  a channel's components restart that device's session.
- **Read model**: an `ArcSwap<Snapshot>` rebuilt after every request and once a
  second (live counters); push events on a `broadcast` channel (`state`, `log`).
  HTTP handlers never block on the engine for reads.
- **API security** (`lightloom_api::server`): listeners on `127.0.0.1` only; every
  request must name a loopback `Host` (`127.0.0.1|localhost|[::1]:<port>`), which
  defeats DNS rebinding; an `Origin`, when present, must be the same loopback
  origin; requests that change state need `Authorization: Bearer <token>`. The token
  (64 hex characters from the OS random source) lives in the user's data directory
  (`%LOCALAPPDATA%\Lightloom\api-token`, `0600` on Unix). The dashboard page gets it
  embedded when lightloomd serves the page; other pages cannot read it (no CORS) and
  cannot fake the `Host`. Read-only requests and the WebSocket streams need no token.
- **Compatible listeners** (16038 REST in SignalRGB's envelope, 16034
  `POST /canvas/event`) start only while no other process listens on their port.
  The check reads the TCP listener table (`GetExtendedTcpTable`): SignalRGB listens
  on the IPv6 wildcard with address reuse, so a bind probe always succeeds on
  Windows and a `127.0.0.1` listener would silently take its IPv4 clients. The
  daemon tries again every 30 s. Compat mutations need the token unless
  `compat_open_mutations`; canvas events are open unless configured otherwise.
- **Dashboard**: Svelte 5 + TypeScript + Vite, built to static files (`ui/dist`) and
  served by lightloomd from `ui/` next to its executable (or the source tree in
  development builds, or `[ui] dir`). It is plain files, not compiled into the
  binary: the daemon builds without Node.js, and the installer ships the files.
  Content-Security-Policy: `default-src 'self'`, no inline script.
- **Single instance**: a lock file in the data directory (`File::try_lock`); the
  native port is bound before any device is touched, so a second daemon on another
  data directory fails before it drives hardware.
- **Paths**: data (database, token, renderer work files) in `%LOCALAPPDATA%\Lightloom`,
  configuration and the user's own plugins/effects/components in
  `%APPDATA%\Lightloom` (XDG equivalents elsewhere); `--home DIR` puts both in one
  place. `config.toml` is read-only for the daemon; choices made in the dashboard
  live in the database.
- **Shutdown**: Ctrl-C, console close, log-off, system shutdown or
  `POST /v1/shutdown` stop every session first, so plugins restore their devices.

## Consequences

- All state changes are serialized; a plugin stuck in `Shutdown()` delays requests
  up to the manager's stop timeout (10 s). Reads stay instant.
- Any local process of the same user can read the token file and use the API; the
  boundary is the user account, as for every per-user daemon.
- The dashboard and the daemon version together; the snapshot is the contract
  (`crates/lightloom-api/src/model.rs`, mirrored in `ui/src/lib/types.ts`).
- Without the renderer executable, effects fall back to a rainbow and the snapshot
  says why.

## Alternatives considered

- **Tauri or another webview shell as the main UI**: extra runtime and a second
  process on the critical path; kept as an optional tray shell for later.
- **Embedding the dashboard in the binary** (`include_dir`/`rust-embed`): forces
  Node.js into every Rust build or a placeholder dance; files next to the binary
  are simpler and the installer ships them anyway.
- **Async engine on tokio**: plugin sessions are threads that block in device I/O;
  one sync thread owning them is simpler to reason about than locks around every
  manager call.
- **Session cookies instead of a bearer token**: needs login flows and CSRF tokens
  for the same protection the Host/Origin checks plus a token header give.
