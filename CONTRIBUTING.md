<!-- SPDX-License-Identifier: Apache-2.0 OR MIT -->
# Contributing to Lightloom

Thank you for helping. Lightloom is pre-alpha; the most useful contributions right
now are protocol facts for the Host API spec, first-party plugins, conformance
fixtures and reviews of the architecture decisions in `docs/adr/`.

Everything below is a rule, not a suggestion, because the project's ability to
stay permissively licensed depends on it.

## 1. Clean-room rule

Lightloom is compatible with a proprietary application and coexists with a GPL-2
one. To keep the code base Apache-2.0 OR MIT, provenance of every fact and every
line matters.

**Allowed sources of protocol and API facts**

- Readable plugin, effect, component and macro files that you legitimately have on
  disk (the reference application ships them as plain JavaScript / HTML / JSON).
  You may read them to learn *facts*: which host calls exist, their argument
  shapes, packet layouts, report ids, endpoint tuples. You must **re-express** what
  you learned in your own words and your own code.
- Public documentation (vendor docs, the reference application's public developer
  docs, USB-IF / HID specs, datasheets).
- Your own captures: USB/HID traces (Wireshark + USBPcap, Frida on a process you own)
  of hardware you own, logs of applications you run, registry keys on your machine.
- Open-source projects under licenses on the `deny.toml` allowlist, with attribution.

**Forbidden**

- **No decompiled or disassembled code** from the reference application or any
  other proprietary binary, and no pseudo-code derived from it. String tables and
  log messages are facts; control flow recovered from a decompiler is not.
- **No code from OpenRGB** (GPL-2-only) in any form: no pasted functions, no
  "translated" functions, no copied tables of magic constants with their comments.
  You may read OpenRGB's *documentation* (e.g. its SMBus access notes) to cross-check
  a fact you established independently, and you must say so in the commit message.
- **No WhirlwindFX-authored files in the repository**, not even as test fixtures.
  Tests that need a corpus read it from a user-supplied directory
  (`tests/conformance/corpus/` is git-ignored).
- No copying of plugin code from community repositories whose license you have not
  checked; add them as *sources* in the sources manager instead.

**How to record provenance.** Protocol notes go to `docs/hardware/<device>.md`
with a "Sources" section listing exactly what kind of source each fact came from
(corpus usage, public doc, own capture). Commits that introduce protocol constants
reference that note.

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
  `SPDX-License-Identifier: Apache-2.0 OR MIT` (plugins under `plugins/`: `MIT`;
  effects under `effects/`: `CC0-1.0`).
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

Hardware conformance (`tests/conformance/`) runs on a self-hosted runner against a
user-supplied plugin directory and real devices:

```text
lightloom compat dry-run "<path to your plugin directory>"   # parse + Initialize() report
lightloom bench                                              # frame / sample / write histograms
```

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
