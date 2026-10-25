#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later
#
# Build Lightloom-<version>-x86_64.AppImage with linuxdeploy from a staged Linux build.
#
# Usage: packaging/appimage/build-appimage.sh --stage DIR [--version V] [--out DIR]
#   --stage    output of tools/package/stage-linux.sh (lightloomd, lightloom, ui/, plugins/, ...)
#   --version  version string for the file name (default: the workspace version)
#   --out      where the .AppImage goes (default: dist)
#
# Environment:
#   LINUXDEPLOY         path to an existing linuxdeploy-x86_64.AppImage (skips the download)
#   LINUXDEPLOY_TAG     linuxdeploy release tag to download (default below; pinned, not
#                       "continuous", so two builds of one commit use the same tool)
#   LINUXDEPLOY_SHA256  expected sha256 of the download; checked when set
#
# The AppImage runtime needs FUSE 2 to mount itself. CI and containers without FUSE run
# linuxdeploy with APPIMAGE_EXTRACT_AND_RUN=1, which this script sets.
#
# An AppImage cannot install udev rules, a group or a kernel module: the user runs
# `Lightloom.AppImage --setup-help` once (packaging/appimage/AppRun, docs/linux-porting.md).

set -euo pipefail

repo="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
here="$repo/packaging/appimage"
stage="" version="" out="dist"
while [ $# -gt 0 ]; do
  case "$1" in
    --stage) stage="$2"; shift 2 ;;
    --version) version="$2"; shift 2 ;;
    --out) out="$2"; shift 2 ;;
    -h | --help) sed -n '3,21p' "$0"; exit 0 ;;
    *) echo "unknown argument: $1" >&2; exit 2 ;;
  esac
done
[ -n "$stage" ] || { echo "usage: $0 --stage DIR [--version V] [--out DIR]" >&2; exit 2; }
[ -x "$stage/lightloomd" ] || { echo "not a staged build (no lightloomd): $stage" >&2; exit 1; }
if [ -z "$version" ]; then
  version="$(sed -n 's/^version *= *"\(.*\)"/\1/p' "$repo/Cargo.toml" | head -n1)"
fi

export ARCH=x86_64
export APPIMAGE_EXTRACT_AND_RUN=1
export LINUXDEPLOY_OUTPUT_VERSION="$version"

work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT

if [ -z "${LINUXDEPLOY:-}" ]; then
  tag="${LINUXDEPLOY_TAG:-1-alpha-20250213-2}"
  LINUXDEPLOY="$work/linuxdeploy-x86_64.AppImage"
  curl -fsSL -o "$LINUXDEPLOY" \
    "https://github.com/linuxdeploy/linuxdeploy/releases/download/$tag/linuxdeploy-x86_64.AppImage"
  if [ -n "${LINUXDEPLOY_SHA256:-}" ]; then
    echo "$LINUXDEPLOY_SHA256  $LINUXDEPLOY" | sha256sum -c -
  fi
  chmod +x "$LINUXDEPLOY"
fi

# AppDir: the staged build under usr/lib/lightloom, symlinks in usr/bin, the desktop entry
# and icon. linuxdeploy adds the shared libraries the ELF files need (libudev and glibc
# stay host libraries, per the AppImage excludelist) and the top-level .desktop/.DirIcon.
appdir="$work/Lightloom.AppDir"
lib="$appdir/usr/lib/lightloom"
mkdir -p "$appdir/usr/lib" "$appdir/usr/bin"
cp -a "$stage" "$lib"
for b in lightloomd lightloom; do
  ln -s "../lib/lightloom/$b" "$appdir/usr/bin/$b"
done

# The CEF effect renderer and its runtime (stage-linux.sh) stay out of linuxdeploy's reach:
# linuxdeploy rewrites the rpath of every ELF file it finds under usr/lib (patchelf on libcef.so
# and replacing the renderer's $ORIGIN rpath) and would bundle Chromium's system libraries
# (NSS, which dlopens its own modules, Mesa's libgbm, X11), which must come from the host.
# They are set aside here and put back before the AppImage is written.
cef="$work/cef-runtime"
mkdir -p "$cef"
shopt -s nullglob
for f in "$lib/lightloom-renderer" "$lib"/*.so "$lib"/*.so.* "$lib"/*.pak "$lib"/*.bin "$lib"/*.dat \
  "$lib/vk_swiftshader_icd.json" "$lib/locales"; do
  if [ -e "$f" ]; then mv "$f" "$cef/"; fi
done
shopt -u nullglob

mkdir -p "$out"
out_abs="$(cd "$out" && pwd)"
(
  cd "$work"
  # Deploy only: no --output, so the CEF files can go back in before packaging.
  "$LINUXDEPLOY" \
    --appdir "$appdir" \
    --custom-apprun "$here/AppRun" \
    --desktop-file "$here/lightloom.desktop" \
    --icon-file "$here/lightloom.svg" \
    --deploy-deps-only "$lib"

  if [ -n "$(ls -A "$cef")" ]; then
    mv "$cef"/* "$lib/"
    # The renderer must find libcef.so next to itself (build.rs `$ORIGIN` rpath).
    if ! ldd "$lib/lightloom-renderer" | grep -E 'libcef\.so => /' >/dev/null; then
      echo "lightloom-renderer: libcef.so does not resolve inside the AppDir" >&2
      exit 1
    fi
  fi

  # The AppImage output plugin bundled in linuxdeploy's own AppImage (usr/bin), run on the
  # finished AppDir without a second linuxdeploy pass.
  "$LINUXDEPLOY" --appimage-extract >/dev/null
  OUTPUT="Lightloom-$version-x86_64.AppImage" \
    squashfs-root/usr/bin/linuxdeploy-plugin-appimage --appdir "$appdir"
  mv "Lightloom-$version-x86_64.AppImage" "$out_abs/"
)

# Smoke test: the AppImage starts and its entry points answer without touching devices.
img="$out_abs/Lightloom-$version-x86_64.AppImage"
"$img" --daemon --version
"$img" --cli --version
"$img" --udev-rules | grep -q 'uaccess'
"$img" --systemd-unit | grep -q '^ExecStart=.* --daemon$'
echo "built $img"
