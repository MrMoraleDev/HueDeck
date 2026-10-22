# Dependabot PRs, October 2026

Status on 2026-10-22. All three branches are based on develop `bfc3190`, so each one merges cleanly.

| Branch | Change | Verdict |
|---|---|---|
| `dependabot/cargo/sha2-0.11.1` (`05e3ed0`) | `sha2` 0.10 → 0.11 (workspace pin plus lockfile) | **Merge.** No code changes needed. |
| `dependabot/github_actions/actions/cache-6` (`cca3e5b`) | `actions/cache@v4` → `@v6` in `ci.yml` and `release.yml` | **Merge.** |
| `dependabot/github_actions/actions/setup-python-7` (`32ef044`) | `actions/setup-python@v5` → `@v7` in `ci.yml` | **Merge.** |

## sha2 0.10.9 → 0.11.1

### What changes
- `sha2` now uses `digest` 0.11, built on `hybrid-array` instead of `generic-array`.
- The lockfile gains `digest 0.11.3`, `block-buffer 0.12.1`, `crypto-common 0.2.2`, `cpufeatures 0.3.1`, `const-oid 0.10.2` and `hybrid-array`.
- All of these are licensed Apache-2.0 OR MIT. `cargo-deny` licence policy is unaffected.
- `digest 0.10.7` and its stack stay in the lockfile, because `sha1 0.10` still pulls them in through `axum` (WebSocket) and `tungstenite`.
  - `deny.toml` has `multiple-versions = "warn"`, so the duplicate only produces warnings and does not fail the build.
  - The duplicate goes away when axum and tungstenite move to `sha1 0.11`.
- Breaking changes in the RustCrypto 0.11 line:
  - `GenericArray` is replaced by `hybrid_array::Array`.
  - Some trait items were renamed or moved (`digest::core_api`, `FixedOutput*`, `Output<D>` type paths).
  - The MSRV rises to 1.85 (edition 2024). We use the `stable` toolchain, so this does not matter.

### Our uses
We only call `Sha256::digest(bytes)` and then iterate the output.
- `crates/huedeck-iobroker/src/pins.rs:82`: `Sha256::digest(bytes).iter().map(|b| format!("{b:02x}"))`. `Array` derefs to `[u8]`, so this still compiles.
- `crates/huedeck-renderer/src/snapshot.rs:41`: `for b in Sha256::digest(..)`. `Array` implements `IntoIterator` by value, so this still compiles.
- `crates/huedeck-setup/src/main.rs` uses `huedeck_iobroker::pins::sha256_hex`.
- We never name `GenericArray`, `Output<_>` or any `digest` trait path.

### Verification
I applied the branch's `Cargo.toml` and `Cargo.lock` in the `wt-status-audit` worktree, then reverted them.
- `cargo test -p huedeck-iobroker -p huedeck-renderer -p huedeck-setup` passed. This includes the PawnIO pin test for `sha256_hex(b"abc")` and the snapshot hash tests.
- `cargo clippy` on the same three crates with `--all-targets -D warnings` was clean.

This check is enough to merge. Run the full workspace test in CI after the merge.

## actions/cache v4 → v6
- **v5.0.0** moved the action to the Node 24 runtime, which needs Actions runner ≥ 2.327.1.
- **v6.0.0** moved the code to ESM and upgraded `@actions/cache`. **v6.1.0** added handling for read-only cache access.
- `action.yml@v6` declares `runs.using: node24`. The inputs we use (`path`, `key`) are unchanged.
- Our usage:
  - `ci.yml:105` (renderer-cef job, `windows-latest`) caches the CEF distribution under the key `cef-154.0.34-windows-x86_64`.
  - `release.yml:87` caches the same thing.
- Runner requirement: we only use GitHub-hosted runners, which are always current. We already run `actions/checkout@v7` and `actions/setup-node@v7`, which are Node 24 actions, so nothing new is required.
- If the self-hosted hardware runner from the plan is ever added, it needs runner ≥ 2.327.1.
- Worst case, the first run after the merge misses the cache once and downloads CEF again. The key is unchanged.
- **Recommendation: merge.**

## actions/setup-python v5 → v7
- **v6.0.0**:
  - Moved to Node 24 (runner ≥ 2.327.1).
  - Added `pip-version` and better `.python-version` / `Pipfile` parsing.
  - A missing cache directory now produces a warning instead of an error.
- **v7.0.0**:
  - Moved to ESM.
  - **Removed the `pip-install` input.**
  - **Removed EOL Python versions** from the manifest.
  - Validates and retries manifest fetches.
  - Upgraded `@actions/cache` to 6.2.
- **v7.1.0** added the `mirror` and `mirror-token` inputs.
- `action.yml@v7` declares `runs.using: node24`.
- Our usage: `ci.yml:171` (the `schemas` job, `ubuntu-latest`) with `python-version: "3.13"`, which is supported. It does not use `pip-install` or `cache:`, and installs requirements with its own `pip` step.
- **Recommendation: merge.**

## Merge order
These PRs are independent and touch different lines, so the order does not matter.
- If develop moves first, ask Dependabot to rebase the cargo PR (`@dependabot rebase`) so `Cargo.lock` is regenerated against the new tree. Do not resolve lockfile conflicts by hand.
