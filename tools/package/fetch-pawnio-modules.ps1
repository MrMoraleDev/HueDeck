# SPDX-License-Identifier: GPL-3.0-or-later
<#
.SYNOPSIS
  Fetch the PawnIO modules HueDeck's I/O broker loads (PawnIO.Modules 0.2.11, signed by
  namazso, LGPL-2.1-or-later) with their licence and sources, every file checked against a
  pinned SHA-256, into one directory (the `pawnio-modules` folder of the staged build).

.DESCRIPTION
  - SmbusPIIX4.bin (AMD), SmbusI801.bin (Intel), Echo.bin and COPYING come from the release archive
    https://github.com/namazso/PawnIO.Modules/releases/download/0.2.11/release_0_2_11.zip
    (the archive and each file pinned). The .bin pins are the ones in
    crates/huedeck-iobroker/src/pins.rs; huedeck-setup checks them again before use.
  - The corresponding sources (SmbusPIIX4.p, SmbusI801.p, Echo.p, sleepmode.inc, include/*.inc) come from
    the repository at the 0.2.11 tag's commit, so the LGPL "source next to the object code"
    requirement is met by the installer itself. SOURCE.txt says where they came from.

  Any mismatch stops the script: nothing unverified is ever staged.

.PARAMETER Out
  Destination directory (created; existing files in it are replaced).

.EXAMPLE
  tools/package/fetch-pawnio-modules.ps1 -Out dist/HueDeck/pawnio-modules
#>
param(
  [Parameter(Mandatory = $true)] [string] $Out
)
$ErrorActionPreference = "Stop"
$ProgressPreference = "SilentlyContinue"

$release = "0.2.11"
$commit = "52a7e536dff3e53c96917a28caac5e0fa6510696"
$zipUrl = "https://github.com/namazso/PawnIO.Modules/releases/download/$release/release_0_2_11.zip"
$zipSha = "43608cb89bc84247fef1368a139013f7d043e17db6d6c8dfc9b46bf0905a81f4"
$fromZip = [ordered]@{
  "SmbusPIIX4.bin" = "91f9b4b1c39e3d399ce48477a89d8f6bd3e58a2241064a4daec3a1513dff56e5"
  "Echo.bin"       = "96cb194cbbf1bd3ced0c0d5e2ac8ec55a5f8964b305dac005a30ac9b72787696"
  "SmbusI801.bin"  = "a0f7d066e7efda28c0e754c1f52dbd8dc280d388ba8575b514a80cb67490530c"
  "COPYING"        = "1e7e6bae5a5bde32f1ae5a7c37a082d1ab03cf89354f7f936ac40be9e39a6531"
}
$sources = [ordered]@{
  "SmbusPIIX4.p"         = "59fd3e6eef8c00904459cd314ded061bb15f4a7dda74b334d194ccde38787378"
  "Echo.p"               = "5f1aa25c8aad697976cb77709f78e283651ef977353bfbfc153a5d0a847132be"
  "SmbusI801.p"          = "fb4e3575fb27f3a5ce0c607ac4e1558f948ab08fd98b8a63cf5072da9e8d6c41"
  "sleepmode.inc"        = "77b022c2c1ad79d8aeb940dccfe28a5f5cb4384400cef3634f30cf827344e0cd"
  "include/core.inc"     = "3c3fdb0bb54d48792e5fadf4a17607023a35f25f1c006893a114342bac9d6aff"
  "include/extra.inc"    = "4780c6ba2489264ff5982f2f9a5e2b7f4e8ec8951adfe8ef88e728592f1b2e94"
  "include/native.inc"   = "b526bf69d66ebe00b22ea8609d9b6c2073ba6eeaf0730a2cf32477d0b64ee395"
  "include/ntstatus.inc" = "bcb76abca4bc951f286074b59f64fb950e384a2204d4dfe6d1e0d03502d0bd2a"
  "include/pawnio.inc"   = "0809f5964d7707272398b212879078f62e08bc579a3def0283a1a059cf9e72a4"
  "include/registry.inc" = "ef33b339849f949b07792936f749de8eca1fa72b4348096c5b644b364e0870ad"
  "include/util.inc"     = "ab390f9d9e244c2f7a5932f42a7e79194d100dc4a16549dbc31f9cf4932f3027"
}

function Assert-Sha([string] $Path, [string] $Want) {
  $got = (Get-FileHash -Algorithm SHA256 $Path).Hash.ToLower()
  if ($got -ne $Want) { throw "$Path`: SHA-256 $got, pinned $Want" }
}

New-Item -ItemType Directory -Force $Out | Out-Null
New-Item -ItemType Directory -Force (Join-Path $Out "include") | Out-Null
$work = Join-Path ([System.IO.Path]::GetTempPath()) ("huedeck-pawnio-" + [guid]::NewGuid())
New-Item -ItemType Directory -Force $work | Out-Null
try {
  $zip = Join-Path $work "release.zip"
  Invoke-WebRequest -UseBasicParsing -Uri $zipUrl -OutFile $zip
  Assert-Sha $zip $zipSha
  Expand-Archive -Path $zip -DestinationPath (Join-Path $work "x")
  foreach ($name in $fromZip.Keys) {
    $src = Join-Path $work "x/$name"
    Assert-Sha $src $fromZip[$name]
    Copy-Item $src (Join-Path $Out $name) -Force
  }
  foreach ($name in $sources.Keys) {
    $dest = Join-Path $Out $name
    Invoke-WebRequest -UseBasicParsing -Uri "https://raw.githubusercontent.com/namazso/PawnIO.Modules/$commit/$name" -OutFile $dest
    Assert-Sha $dest $sources[$name]
  }
} finally {
  Remove-Item -Recurse -Force $work -ErrorAction SilentlyContinue
}

$note = @"
PawnIO.Modules $release (https://github.com/namazso/PawnIO.Modules), by namazso,
licensed under the GNU Lesser General Public License 2.1 or later (COPYING).

SmbusPIIX4.bin, SmbusI801.bin, Echo.bin and COPYING are unmodified files of the signed release archive
$zipUrl
SHA-256 $zipSha

The sources in this folder (SmbusPIIX4.p, SmbusI801.p, Echo.p, sleepmode.inc, include/) are the files of
commit $commit (tag $release) of that repository, the source of these modules.
The whole repository: https://github.com/namazso/PawnIO.Modules/tree/$commit

HueDeck's I/O broker loads only these modules, checked against SHA-256 pins
(crates/huedeck-iobroker/src/pins.rs in the HueDeck sources).
"@
[System.IO.File]::WriteAllText((Join-Path $Out "SOURCE.txt"), ($note -replace "`r`n", "`n"))
"fetched PawnIO.Modules $release into $Out"
