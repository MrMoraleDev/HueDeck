#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later
#
# Build lightloom_<version>_amd64.deb and lightloom-<version>-1.x86_64.rpm with nFPM from a
# staged Linux build (tools/package/stage-linux.sh), the same tree as the release tarball.
#
# Usage: packaging/nfpm/build-packages.sh --stage DIR --version V [--out DIR]
#   --stage    output of tools/package/stage-linux.sh
#   --version  the release file version (1.2.3, 1.2.3-rc.1, or <version>-<short sha>); a
#              pre-release suffix becomes `~suffix` inside the packages, so it sorts first
#   --out      where the packages go (default: dist)
#
# Environment:
#   NFPM         path to an nfpm binary (skips the download)
#   NFPM_VERSION nfpm release to download (default below, pinned); its archive is checked
#   NFPM_SHA256  against NFPM_SHA256 (set together with NFPM_VERSION)
#   SOURCE_DATE_EPOCH  file times and the changelog date (default: now)
#
# The package root is laid out in a temporary directory (see nfpm.yaml for the layout) and
# nfpm runs from there. The deb's and the rpm's maintainer scripts are postinstall.sh and
# postremove.sh: group i2c, i2c-dev, udev reload; the user service is never enabled.

set -euo pipefail

repo="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
here="$repo/packaging/nfpm"
app_id="io.github.MrMoraleDev.Lightloom"
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
[ -n "$stage" ] && [ -n "$version" ] || {
  echo "usage: $0 --stage DIR --version V [--out DIR]" >&2
  exit 2
}
[ -x "$stage/lightloomd" ] || { echo "not a staged build (no lightloomd): $stage" >&2; exit 1; }

work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT

if [ -z "${NFPM:-}" ]; then
  nfpm_version="${NFPM_VERSION:-2.47.0}"
  nfpm_sha256="${NFPM_SHA256:-0660ca602b2d2d2ae4781a06c692b3eeb9d437ffea05b831d76e41f4a3188783}"
  archive="$work/nfpm_${nfpm_version}_Linux_x86_64.tar.gz"
  curl -fsSL -o "$archive" \
    "https://github.com/goreleaser/nfpm/releases/download/v$nfpm_version/nfpm_${nfpm_version}_Linux_x86_64.tar.gz"
  echo "$nfpm_sha256  $archive" | sha256sum -c -
  mkdir -p "$work/nfpm"
  tar -xzf "$archive" -C "$work/nfpm" nfpm
  NFPM="$work/nfpm/nfpm"
fi

# The package root.
root="$work/root"
mkdir -p "$root/doc"
cp -a "$stage" "$root/lib"
# The launcher behind the desktop entry: opens the dashboard of the running daemon (the user
# service), or starts lightloomd --open when none runs (packaging/appimage/AppRun).
cp "$repo/packaging/appimage/AppRun" "$root/lib/lightloom-launch"
# Modes from a fixed list, not from the checkout or the build host: directories 0755, the
# four programs 0755, everything else 0644 (shared libraries need no execute bit, and
# lintian and rpmlint flag it).
find "$root/lib" -type d -exec chmod 0755 {} +
find "$root/lib" -type f -exec chmod 0644 {} +
for b in lightloomd lightloom lightloom-renderer lightloom-launch; do
  if [ -e "$root/lib/$b" ]; then chmod 0755 "$root/lib/$b"; fi
done
# The user unit, pointed at the packaged daemon.
sed -e 's|^ExecStart=.*|ExecStart=/usr/bin/lightloomd|' \
  "$stage/packaging/systemd/lightloomd.service" > "$root/lightloomd.service"
grep -q '^ExecStart=/usr/bin/lightloomd$' "$root/lightloomd.service"
# Desktop entry, icons and AppStream metadata under the Flathub app id.
sed -e "s|^Exec=.*|Exec=/usr/lib/lightloom/lightloom-launch|" -e "s|^Icon=.*|Icon=$app_id|" \
  -e '/^X-AppImage-/d' -e '/^# AppImage:/d' -e '/^# Flatpak rewrites/d' \
  "$repo/packaging/appimage/lightloom.desktop" > "$root/$app_id.desktop"
cp "$repo/packaging/appimage/lightloom.svg" "$root/$app_id.svg"
cp "$repo/docs/brand/lightloom-icon-512.png" "$root/$app_id.png"
cp "$repo/packaging/flatpak/$app_id.metainfo.xml" "$root/$app_id.metainfo.xml"
# Docs (the licences live in /usr/lib/lightloom next to the program, and in the copyright
# file or /usr/share/licenses).
install -m 0644 "$stage/README.md" "$root/doc/README.md"
install -m 0644 "$stage/INSTALL-LINUX.md" "$root/doc/INSTALL-LINUX.md"
cp "$here/copyright" "$root/copyright"
install -m 0755 "$here/postinstall.sh" "$root/postinstall.sh"
install -m 0755 "$here/postremove.sh" "$root/postremove.sh"

# One changelog entry for this version (deb: changelog.Debian.gz, rpm: %changelog), under the
# version nfpm gives the package: a pre-release suffix after `~`, release 1.
tilde="~"
pkg_version="${version/-/$tilde}-1"
epoch="${SOURCE_DATE_EPOCH:-$(date +%s)}"
cat > "$root/changelog.yaml" <<EOF
- semver: "$pkg_version"
  date: "$(date -u -d "@$epoch" +%Y-%m-%dT%H:%M:%SZ)"
  packager: "Lightloom contributors <MrMoraleDev@users.noreply.github.com>"
  deb:
    urgency: medium
    distributions:
      - unstable
  changes:
    - note: "Lightloom $version, see https://github.com/MrMoraleDev/Lightloom/releases"
EOF

if [ -n "${SOURCE_DATE_EPOCH:-}" ]; then
  find "$root" -exec touch -h -d "@$SOURCE_DATE_EPOCH" {} +
fi

mkdir -p "$out"
out_abs="$(cd "$out" && pwd)"
deb="$out_abs/lightloom_${version}_amd64.deb"
rpm="$out_abs/lightloom-${version}-1.x86_64.rpm"
(
  cd "$root"
  export LIGHTLOOM_VERSION="$version"
  "$NFPM" package --config "$here/nfpm.yaml" --packager deb --target "$deb"
  "$NFPM" package --config "$here/nfpm.yaml" --packager rpm --target "$rpm"
)
ls -l "$deb" "$rpm"
