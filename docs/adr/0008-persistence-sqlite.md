<!-- SPDX-License-Identifier: Apache-2.0 OR MIT -->
# ADR-0008: Persistence in SQLite (rusqlite, WAL) mirroring the upstream key shapes

- Status: Accepted (2026-10-17); implemented in M4
- Deciders: project owner, architecture panel (synthesis section 7, D10, D14)
- Related: [ADR-0005](0005-signalrgb-compat-profile.md), [docs/host-api/components-channels.md](../host-api/components-channels.md)

## Context

Lightloom must persist device settings, per-endpoint placements on the 320x200
canvas, per-channel component chains (ordered JSON arrays with component ids),
layouts, effect parameters and presets, macros, plugin sources with their pinned
hashes and license acknowledgements, and misc key/value state.

The reference app keeps all of this in the Windows registry under
`HKCU\Software\WhirlwindFX\SignalRgb\...` (devices keyed by `vid:pid:serial`,
channel arrays per channel name, `lighting/endpoint/<uid>/...`, `effects/<id>`).
Lightloom must:

- run on Linux too, so the registry is not an option;
- let the import wizard copy upstream state 1:1 (same uid grammar, same channel
  array shape);
- update dozens of small values atomically from several threads (UI edits, plugin
  `addProperty`, layout editor) without torn files;
- support schema evolution across releases and byte-for-byte export / re-import;
- never write to the registry (D14).

## Decision

- State lives in a single SQLite database `state.sqlite` in the user data directory
  (`%LOCALAPPDATA%\Lightloom` on Windows, `$XDG_DATA_HOME/lightloom` on Linux), opened
  with `rusqlite` 0.40 (`bundled` feature, WAL journal mode, `synchronous=NORMAL`).
- Schema migrations are forward-only and versioned with `rusqlite_migration`,
  applied at daemon start inside a transaction; the daemon refuses to start on a
  database newer than itself.
- Tables: `devices`, `device_settings`, `placements`, `channel_components` (per
  channel, a JSON array mirroring the upstream HKCU shape, including `ComponentId`
  GUIDs), `layouts`, `effects`, `effect_params`, `presets`, `macros`,
  `plugin_sources` (`kind`, `url|path`, `pinned_sha256`,
  `license_text_acknowledged_at`), `kv`.
- Identifiers use the upstream uid grammar (`vid:pid:serial`, `<uid>:<subdevice>`,
  `<uid>:<ChannelName>`, bare component GUID, `I2CBUS:<addr>`, GPU name), so the
  import wizard maps registry keys to rows without translation.
- Static configuration (ports, fps, HAL selection, compat toggles) is a separate
  `config.toml`; secrets (API token) are a separate file ACL'd to the user.
- Export / import are JSON documents (`.hdlayout`, `.hdpreset`, full state export)
  produced by the daemon, not raw database copies; a round trip must be
  byte-for-byte stable (M4 acceptance).
- The import wizard reads HKCU once, with explicit consent, and never writes it.
- All writes go through the engine's persistence module on the tokio side; plugin
  threads post changes over the event bus (no SQLite handle on plugin threads).

## Consequences

Positive:

- One file to back up; atomic multi-row updates; identical behaviour on both OSes.
- Migrations are explicit and testable; `lightloom` CLI can dump and diff state.
- The 1:1 shape with upstream makes the import wizard a mapping, not a conversion.

Negative / accepted costs:

- Not hand-editable like TOML/JSON; mitigated by `lightloom config export` and the
  dashboard.
- `bundled` SQLite adds compile time and ~1 MB to the binary.
- WAL needs the `-wal`/`-shm` side files on the same filesystem; sandboxed packages
  (Flatpak) must keep the data dir writable.
- Forward-only migrations mean downgrades require restoring an export.

## Alternatives considered

- **Windows registry** (upstream): Windows-only, no transactions across keys, no
  diffable export, and D14 forbids registry persistence.
- **One JSON/TOML file per concern**: readable, but concurrent small updates need
  write-rename dances per file, there is no cross-file atomicity, and large layouts
  rewrite whole files at 30 Hz-adjacent cadence during layout drags.
- **Embedded KV stores (`sled`, `redb`)**: fewer tools, no SQL for the dashboard's
  queries, less mature migration story.
- **Postgres / external DB**: absurd for a local daemon.
