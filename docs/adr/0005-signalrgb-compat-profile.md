<!-- SPDX-License-Identifier: Apache-2.0 OR MIT -->
# ADR-0005: SignalRGB-compatible runtime profile (`compat-v1`) over a native plugin format

- Status: Accepted (2026-10-17)
- Deciders: project owner (compatibility decision of 2026-10-17), architecture panel (synthesis D4, D5, D6, D10, D14, D15)
- Related: [ADR-0003](0003-rquickjs-plugin-engine.md), [ADR-0006](0006-license-apache-mit.md), [docs/host-api/](../host-api/), [docs/architecture-synthesis.md](../architecture-synthesis.md) section 4

## Context

The value of the reference ecosystem is not its binary: it is about 560 readable
JavaScript device plugins, about 560 JSON component descriptors (fans and strips on
ARGB headers), HTML lightscripts, macro scripts and community repositories (GitHub /
GitLab addons, SRGBmods, QMK plugins). A manager that implements the same host API
inherits all of that instead of re-reverse-engineering hundreds of USB protocols.

Constraints:

- WhirlwindFX-authored files are proprietary: they must never be shipped in the
  Lightloom repository or installer. Users already have their own copies on disk.
- "SignalRGB" is a trademark of Whirlwind Virtual Realities Inc.; use must be
  nominative ("compatible with").
- Several host-API semantics are only knowable from how plugins *use* the API
  (readable source, logs, public docs), and a few remain uncertain until measured at
  runtime (e.g. `collection` numbering, `read` return length, LED sample point).
- Third-party tools already talk to the reference app's local HTTP endpoints on
  ports 16038 and 16034.

## Decision

- Lightloom defines its own plugin format, **native-v2** (`manifest.json` with id,
  version, SPDX license, devices, permissions; async I/O variants; TypeScript
  typings; Node simulator). The SignalRGB surface is implemented as a **profile of
  it, `compat-v1`**: the same JavaScript host API (exports, `device.*`, `bus.*`,
  `channel(...)`, `service`/`controller`/`discovery`, `@SignalRGB/*` modules), the
  same HTML effect contract (`<meta>` controls, `engine.*`, 320x200 canvas, load
  sequence) and the same component JSON schema.
- The `@SignalRGB/*` import specifiers are accepted verbatim for interoperability and
  aliased as `@lightloom/*` for native plugins.
- Host API facts are frozen in a **Host API v1 (2.5.80-era) spec** in
  `docs/host-api/` (Apache-2.0 OR MIT, see ADR-0006), derived in a clean room (plugin corpus usage, logs, public docs,
  own captures; no decompiled code). Unknowns are marked and resolved in M2/M3 with
  measurements, never guessed silently.
- **No WhirlwindFX file is ever in the repo**. An import wizard points at the
  user's own installation (`plugin_cdn`, bundled `Plugins`, `Documents\WhirlwindFX`,
  addons, effects, components, HKCU channel arrays and layouts), copies into the
  user data directory and marks the imports `unsigned`.
- Community repositories are pre-listed as **plugin sources** in a sources manager:
  per-repo license text must be acknowledged (`license_text_acknowledged_at`),
  content is hash-pinned and updated only on user action with a diff summary.
- First-party plugins (MIT) are written in the compat shape and loaded by exactly the
  same loader as imported ones, so M1 exercises the M2 code path.
- Unknown host members: log `API_GAP` once, then throw a typed error; a corpus
  dry-run harness (`lightloom compat dry-run <dir>`) produces the gap backlog and
  feeds the compat matrix in the docs.
- Local API compatibility: listeners on `:16038` (`/api/v1/*`) and `:16034`
  (`/canvas/event`) with shape-copied responses, **refusing to bind when the
  reference app owns the ports**; mutating compat calls require the Lightloom token by
  default. The native API lives on `:16138`.
- `ConflictingProcesses` are reported with a "close" button, never killed.

## Consequences

Positive:

- Day-one device coverage of the whole ecosystem with zero plugin changes; the M2
  acceptance bar is 100 % of a user corpus parsing and >= 95 % completing
  `Initialize()` under the harness.
- Layouts, channel arrays and presets import 1:1 because persistence mirrors the
  upstream key shapes (ADR-0008).
- Existing integrations against the compat ports keep working.

Negative / accepted costs:

- A moving target: each upstream release can add API members; the API_GAP telemetry
  (local, never uploaded) and the conformance suite are the answer.
- Some semantics stay "verify at runtime" until M2/M3 (listed in the plan's open
  questions); plugins hitting those corners may misbehave until measured.
- Bugs in imported third-party plugins cannot be fixed in-repo; users patch their
  copies or upstream the fix.
- Legal diligence on nominative use and on each community repo's license is ongoing
  work, not a one-off.

## Alternatives considered

- **Own plugin format only**: re-implement hundreds of protocols; years of work and
  no community gravity.
- **Link to or vendor OpenRGB**: GPL-2-only, incompatible with ADR-0006; the OpenRGB
  SDK **wire protocol** is still implemented (server on `:6742`, M6) because a
  protocol is not code.
- **Fork or redistribute the reference plugins**: proprietary, not permitted.
- **A translation layer** that rewrites upstream plugins into native-v2 at import
  time: fragile, obscures diffs, and still needs the full compat semantics.
- **Compat ports fighting for the socket** (bind with `SO_REUSEADDR`): confusing for
  both applications; refusing to bind is explicit.
