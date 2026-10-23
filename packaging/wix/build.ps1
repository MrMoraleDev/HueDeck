# SPDX-License-Identifier: GPL-3.0-or-later
<#
.SYNOPSIS
  Build Lightloom's per-user MSI from a staged build with the WiX Toolset v5.

.DESCRIPTION
  Input is the directory tools/package/stage-windows.ps1 makes (with the PawnIO modules, i.e.
  without -NoPawnIoModules). Needs the `wix` .NET tool and its UI extension, pinned:

    dotnet tool install --global wix --version 5.0.2
    wix extension add -g WixToolset.UI.wixext/5.0.2

  (-InstallWix does both.) The MSI installs to %LOCALAPPDATA%\Programs\Lightloom without
  administrator rights; see lightloom.wxs and packaging/README.md.

  ICE38/ICE64/ICE91 are suppressed: they are written for per-machine packages and flag every
  file component under the user profile (a file key path instead of an HKCU value, no
  RemoveFolder per harvested directory, a per-user location). Windows Installer still
  installs, repairs and removes those files normally.

.PARAMETER Stage
  The staged build (lightloomd.exe, lightloomw.exe, lightloom-setup.exe, lightloom-iobroker.exe,
  pawnio-modules/, ui/, plugins/, ...).

.PARAMETER Version
  Product version x.y.z (default: the workspace version in Cargo.toml; a pre-release suffix is
  dropped, Windows Installer versions are numeric).

.PARAMETER Culture
  en-US (default) or it-IT.

.PARAMETER Out
  The MSI to write (default: dist/Lightloom-<version>-x64[-<culture>].msi).

.PARAMETER InstallWix
  Install the pinned wix tool and UI extension first (CI).

.EXAMPLE
  packaging/wix/build.ps1 -Stage dist/lightloom-x86_64-pc-windows-msvc
#>
param(
  [Parameter(Mandatory = $true)] [string] $Stage,
  [string] $Version = "",
  [ValidateSet("en-US", "it-IT")] [string] $Culture = "en-US",
  [string] $Out = "",
  [switch] $InstallWix
)
$ErrorActionPreference = "Stop"
$WixVersion = "5.0.2"
$repo = (Resolve-Path (Join-Path $PSScriptRoot "../..")).Path

if ($InstallWix) {
  dotnet tool install --global wix --version $WixVersion
  if ($LASTEXITCODE -ne 0) { dotnet tool update --global wix --version $WixVersion }
  if ($LASTEXITCODE -ne 0) { throw "installing wix $WixVersion failed" }
  wix extension add -g "WixToolset.UI.wixext/$WixVersion"
  if ($LASTEXITCODE -ne 0) { throw "adding WixToolset.UI.wixext $WixVersion failed" }
}
if (-not (Get-Command wix -ErrorAction SilentlyContinue)) {
  throw "wix not found: dotnet tool install --global wix --version $WixVersion (or pass -InstallWix)"
}

if (-not $Version) {
  $toml = Get-Content (Join-Path $repo "Cargo.toml") -Raw
  if ($toml -notmatch '(?ms)^\[workspace\.package\].*?^version\s*=\s*"([^"]+)"') { throw "no workspace version in Cargo.toml" }
  $Version = $Matches[1]
}
$Version = ($Version -replace '^v', '') -replace '[-+].*$', ''
if ($Version -notmatch '^\d+\.\d+\.\d+$') { throw "version $Version is not x.y.z" }

$Stage = (Resolve-Path $Stage).Path
foreach ($f in "lightloomd.exe", "lightloomw.exe", "lightloom.exe", "lightloom-renderer.exe", "lightloom-setup.exe",
  "lightloom-iobroker.exe", "ui/index.html", "pawnio-modules/SmbusPIIX4.bin", "pawnio-modules/Echo.bin",
  "pawnio-modules/COPYING", "pawnio-modules/SmbusPIIX4.p", "LICENSE") {
  if (-not (Test-Path (Join-Path $Stage $f))) { throw "the staged build lacks $f" }
}

if (-not $Out) {
  $suffix = if ($Culture -eq "en-US") { "" } else { "-$Culture" }
  $Out = Join-Path $repo "dist/Lightloom-$Version-x64$suffix.msi"
}
New-Item -ItemType Directory -Force (Split-Path -Parent $Out) | Out-Null

$loc = Join-Path $PSScriptRoot ("Lightloom." + $Culture.ToLower() + ".wxl")
wix build `
  -arch x64 `
  -culture $Culture `
  -loc $loc `
  -ext WixToolset.UI.wixext `
  -d "Version=$Version" `
  -d "StageDir=$Stage" `
  -sice ICE38 -sice ICE64 -sice ICE91 `
  -bindpath $PSScriptRoot `
  -intermediatefolder (Join-Path ([System.IO.Path]::GetTempPath()) "lightloom-wix-obj") `
  -o $Out `
  (Join-Path $PSScriptRoot "lightloom.wxs") `
  (Join-Path $PSScriptRoot "LightloomUI.wxs")
if ($LASTEXITCODE -ne 0) { throw "wix build failed ($LASTEXITCODE)" }

$hash = (Get-FileHash $Out -Algorithm SHA256).Hash.ToLower()
"built $Out ({0:N0} MB) SHA-256 $hash" -f ((Get-Item $Out).Length / 1MB)
