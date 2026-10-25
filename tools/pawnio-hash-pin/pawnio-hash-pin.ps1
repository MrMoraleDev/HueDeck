# SPDX-License-Identifier: GPL-3.0-or-later
<#
.SYNOPSIS
  Recompute the SHA-256 and size of PawnIO module files and check them against the two places
  that pin them: crates/lightloom-iobroker/src/pins.rs (what the broker and lightloom-setup
  accept) and tools/package/fetch-pawnio-modules.ps1 (what the packaging stages).

.DESCRIPTION
  With -Dir it hashes the *.bin files in that folder (a staged `pawnio-modules`, or an extracted
  release). Without it, it downloads the PawnIO.Modules release archive (-Release, default the
  one pins.rs names) into a temporary folder and hashes the files in it; the archive's own
  SHA-256 is printed and checked against the script's $zipSha when the release matches.

  Every module pins.rs names must be present with the pinned size and hash, and the script's
  $fromZip table must name the same hash; any difference exits 1. -Emit also prints the Rust
  `Pin` entries and the PowerShell table lines for the files found, to paste when moving to a
  new release (then review: never pin a file that was not checked against the signed release).

  The cargo test crates/lightloom-iobroker/tests/pawnio_pins.rs keeps pins.rs and the script in
  step without the network; this tool checks them against the real files.

.EXAMPLE
  tools/pawnio-hash-pin/pawnio-hash-pin.ps1
.EXAMPLE
  tools/pawnio-hash-pin/pawnio-hash-pin.ps1 -Dir dist/Lightloom/pawnio-modules
.EXAMPLE
  tools/pawnio-hash-pin/pawnio-hash-pin.ps1 -Release 0.2.12 -Emit
#>
param(
  [string] $Dir,
  [string] $Release,
  [switch] $Emit
)
$ErrorActionPreference = "Stop"
$ProgressPreference = "SilentlyContinue"

$root = Resolve-Path (Join-Path $PSScriptRoot "../..")
$pinsRs = Get-Content -Raw (Join-Path $root "crates/lightloom-iobroker/src/pins.rs")
$fetchPs1 = Get-Content -Raw (Join-Path $root "tools/package/fetch-pawnio-modules.ps1")

# pins.rs: MODULES_RELEASE and every `file: "...", size: N, sha256: "..."`.
$pinnedRelease = [regex]::Match($pinsRs, 'MODULES_RELEASE: &str = "([^"]+)"').Groups[1].Value
$pins = [ordered]@{}
foreach ($m in [regex]::Matches($pinsRs, 'file: "([^"]+\.bin)",\s*size: (\d+),\s*sha256: "([0-9a-f]{64})"')) {
  $pins[$m.Groups[1].Value] = @{ Size = [int64]$m.Groups[2].Value; Sha = $m.Groups[3].Value }
}
if ($pins.Count -eq 0) { throw "no module pins found in pins.rs" }

# fetch-pawnio-modules.ps1: $release, $zipSha and the $fromZip table.
$scriptRelease = [regex]::Match($fetchPs1, '\$release = "([^"]+)"').Groups[1].Value
$scriptZipSha = [regex]::Match($fetchPs1, '\$zipSha = "([0-9a-f]{64})"').Groups[1].Value
$table = [regex]::Match($fetchPs1, '(?s)\$fromZip = \[ordered\]@\{(.*?)\}').Groups[1].Value
$scriptPins = @{}
foreach ($m in [regex]::Matches($table, '"([^"]+)"\s*=\s*"([0-9a-f]{64})"')) {
  $scriptPins[$m.Groups[1].Value] = $m.Groups[2].Value
}

if (-not $Release) { $Release = $pinnedRelease }
$failed = $false
if ($scriptRelease -ne $pinnedRelease) {
  Write-Host "pins.rs pins release $pinnedRelease, fetch-pawnio-modules.ps1 fetches $scriptRelease"
  $failed = $true
}

$work = $null
try {
  if (-not $Dir) {
    $work = Join-Path ([System.IO.Path]::GetTempPath()) ("lightloom-pawnio-pin-" + [guid]::NewGuid())
    New-Item -ItemType Directory -Force $work | Out-Null
    $zipName = "release_" + ($Release -replace '\.', '_') + ".zip"
    $zipUrl = "https://github.com/namazso/PawnIO.Modules/releases/download/$Release/$zipName"
    $zip = Join-Path $work $zipName
    Write-Host "downloading $zipUrl"
    Invoke-WebRequest -UseBasicParsing -Uri $zipUrl -OutFile $zip
    $zipSha = (Get-FileHash -Algorithm SHA256 $zip).Hash.ToLower()
    Write-Host "archive SHA-256 $zipSha"
    if ($Release -eq $scriptRelease -and $zipSha -ne $scriptZipSha) {
      Write-Host "  differs from `$zipSha in fetch-pawnio-modules.ps1 ($scriptZipSha)"
      $failed = $true
    }
    $Dir = Join-Path $work "x"
    Expand-Archive -Path $zip -DestinationPath $Dir
  }

  $found = @{}
  foreach ($f in Get-ChildItem -File -Recurse -Path $Dir -Filter *.bin) {
    $found[$f.Name] = @{ Size = $f.Length; Sha = (Get-FileHash -Algorithm SHA256 $f.FullName).Hash.ToLower() }
  }

  $rows = foreach ($name in $pins.Keys) {
    $pin = $pins[$name]
    $got = $found[$name]
    $status = if (-not $got) { "missing" }
      elseif ($got.Sha -ne $pin.Sha -or $got.Size -ne $pin.Size) { "MISMATCH" }
      elseif ($scriptPins[$name] -ne $pin.Sha) { "script differs" }
      else { "ok" }
    if ($status -ne "ok") { $failed = $true }
    [pscustomobject]@{
      File   = $name
      Status = $status
      Size   = if ($got) { $got.Size } else { "" }
      SHA256 = if ($got) { $got.Sha } else { "" }
    }
  }
  $rows | Format-Table -AutoSize | Out-String -Width 200 | Write-Host

  if ($Emit) {
    Write-Host "// pins.rs (PawnIO.Modules $Release)"
    foreach ($name in ($found.Keys | Sort-Object)) {
      $v = $found[$name]
      Write-Host ("pub const X: Pin = Pin {{ name: ""{0}"", file: ""{1}"", size: {2}, sha256: ""{3}"" }};" -f `
        [System.IO.Path]::GetFileNameWithoutExtension($name), $name, $v.Size, $v.Sha)
    }
    Write-Host "# fetch-pawnio-modules.ps1 `$fromZip"
    foreach ($name in ($found.Keys | Sort-Object)) {
      Write-Host ("  ""{0}"" = ""{1}""" -f $name, $found[$name].Sha)
    }
  }
} finally {
  if ($work) { Remove-Item -Recurse -Force $work -ErrorAction SilentlyContinue }
}

if ($failed) {
  Write-Host "the PawnIO pins do not match"
  exit 1
}
Write-Host "every pinned PawnIO module matches (release $Release)"
