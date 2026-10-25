#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later
#
# Stage Lightloom's Linux build in one directory: the daemon, the CLI, the effect renderer
# with its CEF runtime when it was built, the dashboard, the first-party plugins and
# starter effects, the packaging files (systemd user unit, udev rules, sysusers.d,
# modules-load.d) and the license texts. lightloomd finds its resources next to its
# executable (crates/lightloom-daemon/src/paths.rs), and /proc/self/exe resolves symlinks,
# so a symlink in ~/.local/bin or /usr/bin pointing into this directory works.
#
# The Linux counterpart of tools/package/stage-windows.ps1. Used by the release workflow
# (`linux` job) for the tarball and, as the AppDir payload, by
# packaging/appimage/build-appimage.sh.
#
# Usage: tools/package/stage-linux.sh --release DIR --ui DIR --out DIR
#   --release  cargo's release directory (lightloomd, lightloom; lightloom-renderer and the CEF
#              runtime are optional until the Linux renderer lands, WP-L5)
#   --ui       the dashboard build (ui/dist)
#   --out      where to stage (emptied first)
#   --require-renderer  fail when lightloom-renderer is missing

set -euo pipefail

repo="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
release="" ui="" out="" require_renderer=0
while [ $# -gt 0 ]; do
  case "$1" in
    --release) release="$2"; shift 2 ;;
    --ui) ui="$2"; shift 2 ;;
    --out) out="$2"; shift 2 ;;
    --require-renderer) require_renderer=1; shift ;;
    -h | --help) sed -n '3,20p' "$0"; exit 0 ;;
    *) echo "unknown argument: $1" >&2; exit 2 ;;
  esac
done
[ -n "$release" ] && [ -n "$ui" ] && [ -n "$out" ] || {
  echo "usage: $0 --release DIR --ui DIR --out DIR [--require-renderer]" >&2
  exit 2
}

need() { [ -e "$1" ] || { echo "missing: $1" >&2; exit 1; }; }

rm -rf "$out"
mkdir -p "$out"

# Programs.
for b in lightloomd lightloom; do
  need "$release/$b"
  install -m 0755 "$release/$b" "$out/$b"
done

# The renderer and the CEF runtime it loads from its own directory (WP-L5). Optional:
# without it the daemon still drives devices with the native effects.
cef_files="libcef.so chrome-sandbox icudtl.dat v8_context_snapshot.bin resources.pak
  chrome_100_percent.pak chrome_200_percent.pak libEGL.so libGLESv2.so libvk_swiftshader.so
  vk_swiftshader_icd.json libvulkan.so.1 locales"
if [ -f "$release/lightloom-renderer" ]; then
  install -m 0755 "$release/lightloom-renderer" "$out/lightloom-renderer"
  for f in $cef_files; do
    if [ -e "$release/$f" ]; then cp -a "$release/$f" "$out/"; fi
  done
  if [ -n "${CEF_PATH:-}" ]; then
    credits="$(find "$CEF_PATH" -name CREDITS.html -print -quit 2>/dev/null || true)"
    if [ -n "$credits" ]; then
      mkdir -p "$out/THIRD-PARTY"
      cp "$credits" "$out/THIRD-PARTY/CEF-Chromium-CREDITS.html"
    fi
  fi
elif [ "$require_renderer" = 1 ]; then
  echo "missing: $release/lightloom-renderer (--require-renderer)" >&2
  exit 1
else
  echo "note: no lightloom-renderer in $release; staging without the CEF effect renderer" >&2
fi

# Dashboard.
need "$ui/index.html"
cp -a "$ui" "$out/ui"

# First-party plugins: each plugin directory's JavaScript files and manifest, no tests
# or fixtures (same selection as stage-windows.ps1).
mkdir -p "$out/plugins"
for dir in "$repo"/plugins/*/; do
  [ -f "$dir/manifest.json" ] || continue
  name="$(basename "$dir")"
  mkdir -p "$out/plugins/$name"
  find "$dir" -maxdepth 1 -type f \( -name '*.js' -o -name manifest.json \) \
    -exec cp {} "$out/plugins/$name/" \;
done

# Starter effects (CC0) and component descriptors.
cp -a "$repo/effects" "$out/effects"
if [ -d "$repo/components" ]; then cp -a "$repo/components" "$out/components"; fi

# Linux integration files (installed by the user or a package, never by the daemon).
mkdir -p "$out/packaging"
for f in systemd/lightloomd.service udev/70-lightloom.rules udev/README.md \
  sysusers.d/lightloom.conf modules-load.d/lightloom.conf; do
  need "$repo/packaging/$f"
  mkdir -p "$out/packaging/$(dirname "$f")"
  cp "$repo/packaging/$f" "$out/packaging/$f"
done

# Licenses and notices.
for f in LICENSE LICENSE-EXCEPTION.md README.md; do
  need "$repo/$f"
  cp "$repo/$f" "$out/"
done
cp "$repo/docs/linux-porting.md" "$out/INSTALL-LINUX.md"

# Stable metadata for reproducible archives: every file gets the commit time.
if [ -n "${SOURCE_DATE_EPOCH:-}" ]; then
  find "$out" -exec touch -h -d "@$SOURCE_DATE_EPOCH" {} +
fi

echo "staged $out ($(du -sh "$out" | cut -f1))"
