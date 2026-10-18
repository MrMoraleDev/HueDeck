# SPDX-License-Identifier: Apache-2.0 OR MIT
# Source this (Git Bash on Windows) before any cargo command that may (re)run cef-dll-sys's
# build script, i.e. anything building huedeck-renderer with `--features cef`:
#   . crates/huedeck-renderer/tools/cef-env.sh
#
# Puts CMake + Ninja on PATH: cef-dll-sys builds libcef_dll_wrapper with the Ninja generator and
# neither tool is on PATH by default on Windows. Visual Studio (Build Tools) ships both under
# Common7/IDE/CommonExtensions/Microsoft/CMake when the "C++ CMake tools" component is installed.
#
# CEF_PATH is deliberately NOT set here. cef-dll-sys re-runs its build script whenever CEF_PATH
# changes, and without CEF_PATH it downloads CEF into OUT_DIR. Shells that disagree about
# CEF_PATH therefore keep rebuilding (and re-downloading). Either never set it (crate default:
# one ~170 MB download per target dir and profile), or set it persistently for every shell, for
# example to %LOCALAPPDATA%\huedeck\cef. See crates/huedeck-renderer/README.md.
# Opt in for this shell only: HUEDECK_CEF_PATH=... . crates/huedeck-renderer/tools/cef-env.sh

export PATH="$HOME/.cargo/bin:$PATH"

if [ -n "${HUEDECK_CEF_PATH:-}" ]; then
  export CEF_PATH="$HUEDECK_CEF_PATH"
fi

if ! command -v cmake >/dev/null 2>&1 || ! command -v ninja >/dev/null 2>&1; then
  for _hd_vs in "/c/Program Files/Microsoft Visual Studio/2022" "/c/Program Files (x86)/Microsoft Visual Studio/2022" \
                "/c/Program Files/Microsoft Visual Studio/18" "/c/Program Files (x86)/Microsoft Visual Studio/18"; do
    for _hd_ed in BuildTools Community Professional Enterprise; do
      _hd_base="$_hd_vs/$_hd_ed/Common7/IDE/CommonExtensions/Microsoft/CMake"
      if [ -x "$_hd_base/CMake/bin/cmake.exe" ] && [ -x "$_hd_base/Ninja/ninja.exe" ]; then
        export PATH="$_hd_base/CMake/bin:$_hd_base/Ninja:$PATH"
        break 2
      fi
    done
  done
fi

command -v cmake >/dev/null 2>&1 || echo "cef-env: cmake not found (install CMake, or the VS Build Tools 'C++ CMake tools' component)" >&2
command -v ninja >/dev/null 2>&1 || echo "cef-env: ninja not found (install Ninja, or the VS Build Tools 'C++ CMake tools' component)" >&2
echo "cef-env: cmake=$(command -v cmake 2>/dev/null) ninja=$(command -v ninja 2>/dev/null) CEF_PATH=${CEF_PATH:-<unset: cef-dll-sys downloads into OUT_DIR>}"
