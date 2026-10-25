<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# Contributing to Lightloom

Thank you for helping. Lightloom is pre-alpha; the most useful contributions right
now are protocol facts for the Host API spec, first-party plugins, conformance
fixtures and reviews of the architecture decisions in `docs/adr/`.

Everything below is a rule, not a suggestion: Lightloom can only be distributed
under its license if the origin of every fact and every line is clean.

## 1. Where code and protocol facts may come from

Lightloom is licensed GPL-3.0-or-later ([LICENSE](LICENSE), ADR-0010) with an add-on
exception for plugins and effects ([LICENSE-EXCEPTION.md](LICENSE-EXCEPTION.md)).
It interoperates with a proprietary application (SignalRGB) and reuses code of a
GPL one (OpenRGB), so where every fact and every line comes from matters.

**Allowed**

- **Your own captures**: USB/HID traces of hardware you own (`lightloom ... --record`,
  Wireshark + USBPcap, Frida on a process you own), logs of applications you run,
  registry keys on your machine. Most first-party plugins were written this way
  (`plugins/README.md`, `tools/device-probe/`).
- **Public documentation**: vendor documentation and APIs, USB-IF / HID
  specifications, datasheets, the reference application's public developer docs.
- **Readable files you legitimately have on disk** (the reference application
  ships plugins, effects and components as plain JavaScript / HTML / JSON), for
  *facts* only: which host calls exist, their argument shapes, packet layouts,
  report ids, endpoint tuples. Re-express what you learned in your own words and
  your own code.
- **Code from open-source projects under a GPLv3-compatible license, with
  attribution.** This includes **OpenRGB**, whose source files are
  GPL-2.0-or-later and may be distributed as part of Lightloom under
  GPL-3.0-or-later.
  - Check the header of every file you adapt. `GPL-2.0-or-later`,
    `GPL-3.0-or-later`, `LGPL-2.1-or-later`, `LGPL-3.0-or-later` and the permissive
    licenses on the `deny.toml` allowlist are fine; `GPL-2.0-only` (no "or later")
    is **not**.
  - Keep the original copyright lines, and name the source file, its commit and its
    license in the header of the new file (see "Adapting a device from OpenRGB" in
    [plugins/README.md](plugins/README.md)).
  - List the source in the "Sources" section of `docs/hardware/<device>.md` and in
    the commit message (`Adapted from OpenRGB <path> @ <commit>`).

**Forbidden**

- **No SignalRGB / WhirlwindFX code** in any form: no pasted functions, no
  "translated" functions, no copied tables of constants with their comments, and no
  WhirlwindFX-authored files in the repository, not even as test fixtures. Their
  plugin repository is public but has **no license**, so nothing in it may be
  copied. Facts observed on the wire or learned as described above are fine. Tests
  that need the reference corpus read it from a user-supplied directory
  (`tests/conformance/corpus/` is git-ignored).
- **No decompiled or disassembled code** from the reference application or any
  other proprietary binary, and no pseudo-code derived from it. String tables and
  log messages are facts; control flow recovered from a decompiler is not.
- **No code without a license, or under a license that is not compatible with
  GPLv3** (GPL-2.0-only, CDDL, EPL, proprietary vendor SDKs). Community plugin
  repositories are added as *sources* in the sources manager instead.

**How to record provenance.** Protocol notes go to `docs/hardware/<device>.md`
with a "Sources" section listing exactly what each fact came from (own capture,
public doc, corpus usage, adapted project file). Commits that introduce protocol
constants reference that note.

If you are unsure whether something is allowed, open an issue before writing code.

## 2. Developer Certificate of Origin (DCO)

Lightloom does not use a CLA. Every commit must be signed off, which certifies the
[Developer Certificate of Origin 1.1](https://developercertificate.org/): you wrote
the change or have the right to submit it under the project license.

```text
git commit -s -m "feat(hal-windows): parse &colNN collection index from HID paths"
```

produces the trailer `Signed-off-by: Your Name <you@example.com>`. Use a real name
and an address you control. Unsigned commits are not merged; an automated DCO
check is planned for `.github/workflows/ci.yml` (not wired up yet), until then
reviewers check the trailer by hand.

## 3. Commit messages: Conventional Commits

Format: `type(scope): summary` in the imperative, 72 characters or fewer, body
explaining *why* when it is not obvious, footer with `Signed-off-by` and any
`Refs:` / `Closes:` lines.

Types: `feat`, `fix`, `docs`, `test`, `refactor`, `perf`, `build`, `ci`, `chore`,
`revert`. Scopes are crate or directory names without the `lightloom-` prefix:
`hal`, `hal-windows`, `hal-linux`, `hid-desc`, `plugin-host`, `engine`, `render`,
`renderer`, `iobroker`, `api`, `daemon`, `cli`, `prelude`, `plugins`, `effects`,
`docs`, `adr`, `packaging`, `ci`. Breaking changes get a `!` after the scope and a
`BREAKING CHANGE:` footer.

One logical change per commit; keep formatting-only changes separate.

## 4. Code and file conventions

- Rust: `cargo fmt` and `cargo clippy --workspace --all-targets -- -D warnings`
  must pass; edition and MSRV come from `Cargo.toml`; no `unsafe` without a
  `// SAFETY:` comment; new dependencies are added to `[workspace.dependencies]`
  and must pass `cargo deny check`.
- Every file starts with an SPDX header where the format allows:
  `SPDX-License-Identifier: GPL-3.0-or-later` (effects under `effects/`: `CC0-1.0`).
  Files adapted from another project keep that project's copyright lines below it.
- UTF-8, LF line endings, no BOM, final newline (`.editorconfig` and
  `.gitattributes` enforce this; `.cmd`/`.bat`/`.ps1` are the only CRLF files).
- Prelude shims (`prelude/`) and first-party plugins are plain JavaScript with
  `node:test` tests next to them; no build step, no TypeScript in the prelude.
- Scripts under `tools/` and `tests/` that read untrusted files are Python 3.13 run
  with `python -I` or Rust binaries; never run an interpreter from inside a corpus
  directory.
- Host API behaviour that is not yet verified against hardware is marked
  `UNKNOWN` / `PLAUSIBLE` in `docs/host-api/` and must not be presented as fact in
  code comments.

## 5. Running the tests

No hardware is needed for the default suite; the HAL falls back to the mock backend
(`LIGHTLOOM_HAL=mock`, which CI sets).

```text
# Rust (Windows and Linux)
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo deny check                       # licenses, advisories, bans, sources

# Prelude shims and first-party plugins (Node 24; Node >= 21 needs a glob, not a
# bare directory, and matches CI exactly)
node --test "prelude/tests/**/*.test.{js,mjs}"
node --test "plugins/**/*.test.{js,mjs}"

# JSON Schemas against your own corpus (Python 3.13; skipped when no corpus;
# the script lands together with the schema tests, CI skips it while absent)
python -I tests/schemas/validate_corpus.py --skip-if-missing-corpus
```

Conformance (`tests/conformance/run.py`) runs on the reference machine, by hand or
through the manually dispatched `conformance-selfhosted.yml` workflow (self-hosted
runner labelled `self-hosted, windows, lightloom-reference`). It needs no device: a
dry run over a user-supplied plugin directory (100 % must parse, at least 95 % of
`Initialize()` calls must succeed, API gaps are reported), a replay of every recorded
HID trace in `plugins/*/fixtures/`, and optionally the effects PSNR comparison:

```text
cargo build --release -p lightloom-cli
python -I tests/conformance/run.py --lightloom target/release/lightloom.exe \
    --corpus "<path to your plugin directory>" [--traces <your recordings>] \
    [--renderer <lightloom-renderer with cef> --chrome <chrome.exe>]
python -I -m unittest discover -s tests/conformance -p "test_*.py"   # the runner's own tests
```

A trace recorded with `lightloom light <colour> --record FILE` on real hardware goes in
the plugin's `fixtures/` (any `*.json` name; the runner tells traces from packet
expectations by their content). On real devices, `lightloom bench` prints frame, sample
and write histograms.

Linux builds need `libudev-dev`, `libusb-1.0-0-dev` and `pkg-config`. CEF binaries
are fetched by the `cef` crate's build script on first build; set
`CEF_PATH` / use `export-cef-dir` to reuse a local copy.

## 6. Pull requests

- Open an issue or discussion first for anything touching `docs/adr/`,
  `lightloom-protocol`, the broker, or the Host API spec.
- Keep PRs small; include tests (mock-HAL golden packets for plugins, record/replay
  fixtures for compat behaviour, schema tests for JSON).
- State in the PR description where every protocol fact came from (corpus usage,
  public doc, own capture); a PR template with a provenance checklist under
  `.github/` is planned.
- CI must be green on both OSes; `cargo deny` failures are never waived.

## 7. Security issues

Do not open public issues for vulnerabilities; follow [SECURITY.md](SECURITY.md).

## 8. Conduct

This project follows the [Contributor Covenant 2.1](CODE_OF_CONDUCT.md).
