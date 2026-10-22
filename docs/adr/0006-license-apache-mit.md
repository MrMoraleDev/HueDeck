<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# ADR-0006: Licensing — Apache-2.0 OR MIT for code, MIT for first-party plugins, CC0 for starter effects

- Status: **Superseded** by [ADR-0010](0010-license-gpl-3.md) (2026-10-18); accepted 2026-10-17 (user decision)
- Deciders: project owner, architecture panel (synthesis D11)
- Related: [ADR-0004](0004-localsystem-iobroker-pawnio.md), [ADR-0005](0005-signalrgb-compat-profile.md), [CONTRIBUTING.md](../../CONTRIBUTING.md), `deny.toml`

## Context

Lightloom wants the widest possible adoption of its runtime, its Host API spec and
its plugin SDK, including by closed-source vendor tools and by other open-source
managers. At the same time:

- OpenRGB (the other large open RGB project) is GPL-2-only; copying its code would
  force the whole core to GPL-2 and block permissive reuse. (Correction,
  2026-10-18: OpenRGB's source files are GPL-2.0-or-later; see ADR-0010.)
- PawnIO modules are Pawn source run by a GPL-2 driver whose exception covers only
  the IOCTL interface; official PawnIO builds load only modules that are LGPL-2.1,
  merged into PawnIO.Modules and signed by its maintainer.
- The Rust ecosystem convention is dual Apache-2.0 / MIT, which maximizes
  compatibility with the crates we depend on and gives an explicit patent grant.
- Community plugin repositories come under their own licenses and must be consumed
  as *sources*, with the license acknowledged, not vendored.

## Decision

- All code in this repository (core crates, CLI, renderer, broker, SDK, tooling,
  schemas) and the Host API specification in `docs/host-api/` are licensed
  **Apache-2.0 OR MIT** at the recipient's choice. `LICENSE-APACHE` and `LICENSE-MIT`
  live at the repository root.
- First-party device plugins under `plugins/` are **MIT** (each with its own
  `LICENSE`), so they can be copied into any other manager, including proprietary
  ones, with minimal friction.
- Starter effects under `effects/` are **CC0-1.0**, so users can remix them without
  attribution.
- Pawn modules the project needs beyond the official set are written under
  **LGPL-2.1** and upstreamed to PawnIO.Modules (the only way they load on official
  PawnIO builds), never kept in this tree. The signed module blobs Lightloom uses are
  separate LGPL-2.1 files shipped with their license text and source location
  (amended 2026-10-18 after the M5 research brief; previously "a separate GPL-2
  repository").
- `cargo-deny` enforces a permissive-only allowlist in-tree (MIT, Apache-2.0,
  BSD-2/3, ISC, Zlib, MPL-2.0, Unicode, CC0, 0BSD, BSL-1.0, OpenSSL); GPL, LGPL
  and AGPL crates are denied.
- Contributions are accepted under the same terms via the **Developer Certificate of
  Origin** (`Signed-off-by`), not a CLA.
- Every file carries an SPDX header where the format allows
  (`SPDX-License-Identifier: GPL-3.0-or-later`; plugins `MIT`; effects `CC0-1.0`;
  the Code of Conduct is `CC-BY-4.0` as published by its authors).
- Imported third-party files (reference-app plugins, community repos) are stored in
  the user's data directory, marked `imported` / `unsigned`, and never relicensed.

## Consequences

Positive:

- Maximum reach for the spec, SDK and plugins; vendors can adopt the Host API
  under the same two licenses every Rust crate they already use carries.
- Apache-2.0 provides an express patent license for contributors' patents;
  MIT keeps the option for GPL-2-only consumers to use the code.
- Matches the licenses of every pinned crate; no license laundering needed.

Negative / accepted costs:

- No copyleft protection: a vendor may ship a closed fork of the runtime. Accepted
  because the ecosystem value is in open plugins and an open spec.
- OpenRGB code can never be pasted, even small helpers; protocol facts must be
  re-expressed (see the clean-room rule in `CONTRIBUTING.md`).
- Two license files and per-directory sub-licenses must be kept in sync in
  packaging (MSI, AppImage, Flatpak metadata, SBOM).

## Alternatives considered

- **GPL-3** (proposal B): would wall the core off from OpenRGB (GPL-2-only) *and*
  from permissive consumers at the same time; also, B's premise that QtWebEngine has
  no GPL-2-compatible option was wrong, so the argument for GPL-3 collapsed.
- **GPL-2**: would allow OpenRGB code reuse, but makes the spec and SDK unattractive
  to vendors and conflicts with several Apache-2.0-only crates (patent clause).
- **MPL-2.0**: file-level copyleft; a reasonable middle ground but unfamiliar to the
  Rust ecosystem and no clearer benefit for a plugin-driven project.
- **MIT only**: no patent grant.
- **AGPL**: irrelevant for a local daemon and hostile to adoption.
