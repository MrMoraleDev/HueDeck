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
# Needs python3 with aiohttp and tomlkit (cargo generator) and pipx or pip (node generator).
# FLATPAK_BUILDER_TOOLS: an existing flatpak-builder-tools checkout (skips the clone).
# FLATPAK_BUILDER_TOOLS_REV: the commit to clone (pin it for reproducible lists).

set -euo pipefail

repo="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
here="$repo/packaging/flatpak"

tools="${FLATPAK_BUILDER_TOOLS:-}"
if [ -z "$tools" ]; then
  tools="$(mktemp -d)"
  trap 'rm -rf "$tools"' EXIT
  git clone --quiet https://github.com/flatpak/flatpak-builder-tools.git "$tools"
  if [ -n "${FLATPAK_BUILDER_TOOLS_REV:-}" ]; then
    git -C "$tools" checkout --quiet "$FLATPAK_BUILDER_TOOLS_REV"
  fi
fi

python3 "$tools/cargo/flatpak-cargo-generator.py" "$repo/Cargo.lock" \
  -o "$here/cargo-sources.json"

if ! command -v flatpak-node-generator >/dev/null 2>&1; then
  python3 -m pip install --user "$tools/node"
fi
flatpak-node-generator npm "$repo/ui/package-lock.json" -o "$here/node-sources.json"

echo "wrote $here/cargo-sources.json and $here/node-sources.json"
