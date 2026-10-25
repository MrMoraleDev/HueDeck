#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later
#
# Generate the Flatpak offline sources from the lock files:
#   cargo-sources.json  every crate in Cargo.lock (flatpak-cargo-generator)
#   node-sources.json   every npm package in ui/package-lock.json (flatpak-node-generator)
#
# flatpak-builder builds without network access, so both lists must match the lock files
# of the tree being built: rerun this after any Cargo.lock or package-lock.json change.
#
# Needs git and python3 with the venv module. The generators and their Python dependencies
# (aiohttp, tomlkit, PyYAML) go into a throwaway virtualenv, so this also runs where pip may
# not install into the system Python (PEP 668: Debian 12, Ubuntu 24.04).
# FLATPAK_BUILDER_TOOLS: an existing flatpak-builder-tools checkout (skips the clone).
# FLATPAK_BUILDER_TOOLS_REV: the commit to clone (default below: pinned, so the same lock
# files always give the same lists).

set -euo pipefail

repo="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
here="$repo/packaging/flatpak"

work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT

tools="${FLATPAK_BUILDER_TOOLS:-}"
if [ -z "$tools" ]; then
  tools="$work/flatpak-builder-tools"
  git clone --quiet https://github.com/flatpak/flatpak-builder-tools.git "$tools"
  git -C "$tools" checkout --quiet     "${FLATPAK_BUILDER_TOOLS_REV:-74697c75b630d7330e77250fc13cb5ea688d9479}"
fi

python3 -m venv "$work/venv"
"$work/venv/bin/pip" install --quiet --disable-pip-version-check aiohttp tomlkit "$tools/node"

"$work/venv/bin/python" "$tools/cargo/flatpak-cargo-generator.py" "$repo/Cargo.lock"   -o "$here/cargo-sources.json"
"$work/venv/bin/flatpak-node-generator" npm "$repo/ui/package-lock.json"   -o "$here/node-sources.json"

echo "wrote $here/cargo-sources.json and $here/node-sources.json"
