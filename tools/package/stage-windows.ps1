# SPDX-License-Identifier: GPL-3.0-or-later
<#
.SYNOPSIS
  Stage HueDeck's portable Windows build in one directory: the daemon, the CLI, the effect
  renderer with its CEF runtime, the dashboard, the first-party plugins and starter effects,
  and the license texts. huedeckd finds everything next to its executable.

.PARAMETER Release
  Cargo's release directory holding huedeckd.exe, huedeck.exe, huedeck-renderer.exe (built
  with --features cef) and the CEF runtime files the cef build copied next to them.

.PARAMETER Ui
  The dashboard build (ui/dist: index.html and assets/).

.PARAMETER Cef
  The CEF binary distribution the renderer was built against (for CREDITS.html, the Chromium
  third-party notices that must ship with it). Default: the directory under $env:CEF_PATH that
  holds CREDITS.html.

.PARAMETER Out
  Where to stage (emptied first).

.EXAMPLE
  tools/package/stage-windows.ps1 -Release target/release -Ui ui/dist -Out dist/HueDeck
#>
param(
  [Parameter(Mandatory = $true)] [string] $Release,
  [Parameter(Mandatory = $true)] [string] $Ui,
  [string] $Cef = "",
  [string] $Out = "dist/HueDeck"
)
$ErrorActionPreference = "Stop"
$repo = (Resolve-Path (Join-Path $PSScriptRoot "../..")).Path

function Copy-Required([string] $From, [string] $To) {
  if (-not (Test-Path $From)) { throw "missing: $From" }
  Copy-Item -Path $From -Destination $To -Recurse -Force
}

if (Test-Path $Out) { Remove-Item -Recurse -Force $Out }
New-Item -ItemType Directory -Force $Out | Out-Null

# Programs.
foreach ($exe in "huedeckd.exe", "huedeck.exe", "huedeck-renderer.exe") {
  Copy-Required (Join-Path $Release $exe) $Out
}

# The CEF runtime huedeck-renderer loads from its own directory.
$cefFiles = "libcef.dll", "chrome_elf.dll", "d3dcompiler_47.dll", "vk_swiftshader.dll",
  "vk_swiftshader_icd.json", "icudtl.dat", "v8_context_snapshot.bin", "resources.pak",
  "chrome_100_percent.pak", "chrome_200_percent.pak", "locales"
foreach ($f in $cefFiles) {
  Copy-Required (Join-Path $Release $f) $Out
}

# Every staged exe must start on a clean Windows: system DLLs and the shipped CEF runtime only,
# no VC++ runtime (the build links the CRT statically, see .cargo/config.toml).
& (Join-Path $PSScriptRoot "check-imports.ps1") $Out

# Dashboard.
if (-not (Test-Path (Join-Path $Ui "index.html"))) { throw "no dashboard build in $Ui" }
Copy-Required $Ui (Join-Path $Out "ui")

# First-party plugins: each plugin directory's JavaScript files and manifest, no tests or fixtures.
$plugins = Join-Path $Out "plugins"
New-Item -ItemType Directory -Force $plugins | Out-Null
foreach ($dir in Get-ChildItem -Directory (Join-Path $repo "plugins")) {
  if (-not (Test-Path (Join-Path $dir.FullName "manifest.json"))) { continue }
  $dest = Join-Path $plugins $dir.Name
  New-Item -ItemType Directory -Force $dest | Out-Null
  foreach ($file in Get-ChildItem -File $dir.FullName) {
    if ($file.Extension -eq ".js" -or $file.Name -eq "manifest.json") {
      Copy-Item $file.FullName $dest
    }
  }
}

# Starter effects (CC0) and component descriptors.
Copy-Required (Join-Path $repo "effects") (Join-Path $Out "effects")
if (Test-Path (Join-Path $repo "components")) {
  Copy-Item -Recurse -Force (Join-Path $repo "components") (Join-Path $Out "components")
}

# Licenses and notices.
foreach ($f in "LICENSE", "LICENSE-EXCEPTION.md", "README.md") {
  Copy-Required (Join-Path $repo $f) $Out
}
if (-not $Cef) {
  if (-not $env:CEF_PATH) { throw "pass -Cef or set CEF_PATH (for CREDITS.html)" }
  $credits = Get-ChildItem -Recurse -File -Filter "CREDITS.html" $env:CEF_PATH | Select-Object -First 1
  if (-not $credits) { throw "no CREDITS.html under $($env:CEF_PATH)" }
  $Cef = $credits.DirectoryName
}
$third = Join-Path $Out "THIRD-PARTY"
New-Item -ItemType Directory -Force $third | Out-Null
Copy-Required (Join-Path $Cef "CREDITS.html") (Join-Path $third "CEF-Chromium-CREDITS.html")

$size = (Get-ChildItem -Recurse -File $Out | Measure-Object -Property Length -Sum).Sum
"staged $Out ({0:N0} MB)" -f ($size / 1MB)
