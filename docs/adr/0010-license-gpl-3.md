<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# ADR-0010: Licensing — GPL-3.0-or-later with an add-on exception

- Status: Accepted (2026-10-18, user decision); supersedes [ADR-0006](0006-license-apache-mit.md)
- Deciders: project owner
- Related: [LICENSE](../../LICENSE), [LICENSE-EXCEPTION.md](../../LICENSE-EXCEPTION.md), [CONTRIBUTING.md](../../CONTRIBUTING.md), `deny.toml`, [ADR-0004](0004-localsystem-iobroker-pawnio.md), [ADR-0005](0005-signalrgb-compat-profile.md)

## Context

ADR-0006 chose Apache-2.0 OR MIT to maximise reuse of the runtime, the Host API
spec and the plugin SDK, closed-source tools included. Since then the direction
of the project changed (2026-10-18):

- HueDeck is a **standalone** manager: users download it and control their lights
  with no other program installed. Device coverage must come from HueDeck's own
  plugin library, not from SignalRGB's plugins or a running OpenRGB.
- Writing every protocol from our own traffic captures works (four device families
  were written that way) but is slow. OpenRGB already drives hundreds of device
  families, and its source files are **GPL-2.0-or-later** (their SPDX headers,
  checked 2026-10-18; ADR-0006 called it GPL-2-only, which was wrong). Other open
  projects with device knowledge, such as liquidctl, are GPL-3.0-or-later.
- HueDeck exists to be free of telemetry, ads and accounts. A permissive license
  lets anyone ship a closed fork that adds them back; copyleft does not.
- All code in the repository so far was written for HueDeck, so it can be
  relicensed.
- Community plugins and effects in the SignalRGB format come under many licenses,
  or none, and users must keep being able to run them in HueDeck.

## Decision

- All code and documentation in this repository is licensed
  **GPL-3.0-or-later**; `LICENSE` holds the GPLv3 text. This covers the crates
  (core, CLI, daemon, renderer, broker), the dashboard in `ui/`, the prelude shims,
  the schemas, the tools, the Host API spec in `docs/host-api/` and the first-party
  device plugins in `plugins/`. Files carry `SPDX-License-Identifier:
  GPL-3.0-or-later`.
- Not covered by that single license: the starter effects in `effects/` and the
  probe effect `tools/device-probe/coords.html` stay **CC0-1.0**; the Code of
  Conduct stays **CC-BY-4.0**.
- An **add-on exception** (`LICENSE-EXCEPTION.md`, an additional permission under
  GPLv3 section 7) lets device plugins, effects, components and other add-ons that
  use HueDeck only through its add-on interfaces (plugin host API, effect contract,
  data formats, local APIs) carry any license, and lets HueDeck load them and be
  conveyed together with them. An add-on that copies HueDeck code is a covered
  work.
- **OpenRGB code may be adapted** into HueDeck plugins, one source file at a time,
  when that file's header says GPL-2.0-or-later (or another GPLv3-compatible
  license). The adapted plugin keeps the original copyright lines, names the
  OpenRGB file, commit and license, and is distributed as part of HueDeck under
  GPL-3.0-or-later, which the "or later" clause permits. GPL-2.0-only files are
  never used. How to do it: `plugins/README.md`.
- **SignalRGB / WhirlwindFX code stays excluded** in any form, translations
  included: its public plugin repository has no license. Facts observed on the
  wire or read from the user's own installation (packet layouts, host API shapes)
  remain usable, as before.
- `cargo-deny` allows only licenses compatible with GPLv3: the permissive set of
  ADR-0006 plus GPL-3.0-or-later for HueDeck's own crates. The OpenSSL license
  (GPL-incompatible, used by no dependency) leaves the allowlist. On 2026-10-18
  every dependency is available under MIT, Apache-2.0, BSD-2-Clause,
  BSD-3-Clause, ISC, Zlib, 0BSD, BSL-1.0, MPL-2.0, Unicode-3.0 or the Unlicense,
  all GPLv3-compatible; the two build-only exceptions (`bzip2-1.0.6`,
  `CDLA-Permissive-2.0`, never shipped) are unchanged. CEF and Chromium
  (BSD-3-Clause plus their third-party licenses) ship as separate libraries that
  `huedeck-renderer` loads, as in other GPL applications that embed CEF.
- Contributions are accepted under GPL-3.0-or-later with the Developer Certificate
  of Origin, as before; no CLA.
- PawnIO modules are unchanged: LGPL-2.1, upstreamed to PawnIO.Modules (ADR-0004).

## Consequences

Positive:

- Device coverage can grow quickly by adapting OpenRGB (and other GPL) device
  code, with attribution, into first-party plugins.
- Nobody can distribute a closed HueDeck build with telemetry or ads: whoever
  distributes HueDeck must provide its source under the same license.
- Third-party plugins and effects keep their own licenses (add-on exception), and
  SignalRGB-format community plugins keep running.
- Selling official builds, donations and sponsorships remain possible: the GPL
  permits sale, and it does not license the "HueDeck" name.

Negative / accepted costs:

- Closed-source products can no longer embed HueDeck's code. They can still talk
  to it through the local APIs, or ship add-ons.
- Code adapted from OpenRGB stays GPL; changing its license later would mean
  rewriting it. Dual licensing is only possible for code the project owns
  entirely.
- First-party plugins can no longer be copied into permissively licensed
  projects.
- Every release must carry `LICENSE` and `LICENSE-EXCEPTION.md` and point to the
  matching source (the release workflow stages both files).

## Alternatives considered

- **Keep Apache-2.0 OR MIT** (ADR-0006): no OpenRGB reuse; device coverage only
  from our own captures and public specifications.
- **Hybrid**: a permissive core plus a separate GPL device pack adapted from
  OpenRGB. Legal, but two licensing regimes in one product, and the copyleft would
  only protect the pack.
- **GPL-2.0-or-later**: also allows OpenRGB reuse, but binaries that include
  Apache-2.0 dependencies are effectively GPLv3 anyway; stating GPL-3.0-or-later
  says so plainly and adds GPLv3's patent and anti-lockdown terms.
- **AGPL-3.0**: adds nothing for a daemon that only listens on loopback.
