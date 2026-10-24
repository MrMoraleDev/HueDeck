# SPDX-License-Identifier: GPL-3.0-or-later
<#
.SYNOPSIS
  Build (and optionally sign) the Lightloom identity package: a sparse MSIX with
  external location that makes lightloomd.exe a Windows Dynamic Lighting ambient
  lighting controller.

.DESCRIPTION
  Stages AppxManifest.xml (from AppxManifest.xml.in) and the logo assets
  (rendered from packaging/wix/lightloom.ico), packs them with MakeAppx.exe
  (/nv: a sparse package has no payload to validate) and, when -Pfx is given,
  signs the package with SignTool.exe. With -EmbedIn it also merges the msix
  side-by-side manifest (lightloomd.exe.manifest.in) into a built lightloomd.exe
  with mt.exe, keeping the manifest link.exe embedded.

  This script never installs, registers or trusts anything. It prints the
  commands that would, for the user to run.

.PARAMETER Publisher
  The exact subject DN of the signing certificate (for example
  "CN=Lightloom Dev"). For an unsigned dev package on Windows 11 append
  ", OID.2.25.311729368913984317654407730594956997722=1".

.EXAMPLE
  ./build.ps1 -Publisher "CN=Lightloom Dev" -Pfx dev.pfx -PfxPassword (Read-Host -AsSecureString) `
      -EmbedIn ..\..\target\release\lightloomd.exe
#>
[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)] [string] $Publisher,
    [string] $Version = "0.6.0.0",
    [string] $Name = "Lightloom.Lighting",
    # Path of lightloomd.exe relative to the external location (the install directory).
    [string] $Executable = "lightloomd.exe",
    [string] $OutDir = (Join-Path $PSScriptRoot "out"),
    [string] $Pfx,
    [SecureString] $PfxPassword,
    # A built lightloomd.exe to embed the msix side-by-side manifest into.
    [string] $EmbedIn,
    # Stage the manifest and assets only (no MakeAppx, SignTool or mt.exe).
    [switch] $StageOnly
)

$ErrorActionPreference = "Stop"

if ($Version -notmatch '^\d+\.\d+\.\d+\.\d+$') { throw "Version must be a.b.c.d, got '$Version'" }
if ($Publisher -notmatch '^CN=') { throw "Publisher must be a subject DN starting with CN=" }

function Escape-Xml([string] $s) { [System.Security.SecurityElement]::Escape($s) }

function Fill([string] $template, [string] $target) {
    $text = Get-Content -Raw -Encoding UTF8 (Join-Path $PSScriptRoot $template)
    $text = $text.Replace("@NAME@", (Escape-Xml $Name)).Replace("@PUBLISHER@", (Escape-Xml $Publisher))
    $text = $text.Replace("@VERSION@", $Version).Replace("@EXECUTABLE@", (Escape-Xml $Executable))
    [System.IO.File]::WriteAllText($target, $text.Replace("`r`n", "`n"), (New-Object System.Text.UTF8Encoding $false))
}

function Find-SdkTool([string] $exe) {
    $cmd = Get-Command $exe -ErrorAction SilentlyContinue
    if ($cmd) { return $cmd.Source }
    $kits = Join-Path ${env:ProgramFiles(x86)} "Windows Kits\10\bin"
    $hit = Get-ChildItem -Path $kits -Filter $exe -Recurse -ErrorAction SilentlyContinue |
        Where-Object { $_.FullName -match '\\x64\\' } | Sort-Object FullName -Descending | Select-Object -First 1
    if (-not $hit) { throw "$exe not found: install the Windows SDK or put it on PATH" }
    return $hit.FullName
}

function Write-Logo([string] $path, [int] $size) {
    Add-Type -AssemblyName System.Drawing
    # Icon.ToBitmap cannot read PNG-compressed entries (usually the 256 px one): take
    # the largest bitmap entry that converts.
    $src = $null
    foreach ($s in 128, 64, 48, 32, 16) {
        $ico = New-Object System.Drawing.Icon((Join-Path $PSScriptRoot "..\wix\lightloom.ico"), $s, $s)
        try { $src = $ico.ToBitmap() } catch { $src = $null } finally { $ico.Dispose() }
        if ($src) { break }
    }
    if (-not $src) { throw "cannot read packaging/wix/lightloom.ico" }
    $bmp = New-Object System.Drawing.Bitmap($size, $size)
    $g = [System.Drawing.Graphics]::FromImage($bmp)
    $g.InterpolationMode = [System.Drawing.Drawing2D.InterpolationMode]::HighQualityBicubic
    $g.DrawImage($src, 0, 0, $size, $size)
    $g.Dispose(); $src.Dispose()
    $bmp.Save($path, [System.Drawing.Imaging.ImageFormat]::Png)
    $bmp.Dispose()
}

# --- stage -------------------------------------------------------------------
$stage = Join-Path $OutDir "stage"
if (Test-Path $stage) { Remove-Item -Recurse -Force $stage }
New-Item -ItemType Directory -Force (Join-Path $stage "assets") | Out-Null
Fill "AppxManifest.xml.in" (Join-Path $stage "AppxManifest.xml")
Write-Logo (Join-Path $stage "assets\StoreLogo.png") 50
Write-Logo (Join-Path $stage "assets\Square150x150Logo.png") 150
Write-Logo (Join-Path $stage "assets\Square44x44Logo.png") 44
$sxs = Join-Path $OutDir "lightloomd.exe.manifest"
Fill "lightloomd.exe.manifest.in" $sxs
Write-Host "staged $stage and $sxs"
if ($StageOnly) { return }

# --- pack and sign -----------------------------------------------------------
$msix = Join-Path $OutDir "lightloom-identity.msix"
& (Find-SdkTool "MakeAppx.exe") pack /o /d $stage /nv /p $msix
if ($LASTEXITCODE -ne 0) { throw "MakeAppx failed ($LASTEXITCODE)" }
if ($Pfx) {
    $plain = if ($PfxPassword) { [System.Net.NetworkCredential]::new("", $PfxPassword).Password } else { "" }
    & (Find-SdkTool "SignTool.exe") sign /fd SHA256 /f $Pfx /p $plain $msix
    if ($LASTEXITCODE -ne 0) { throw "SignTool failed ($LASTEXITCODE)" }
} else {
    Write-Warning "not signed: registration needs a trusted signature, or an unsigned-publisher DN and -AllowUnsigned"
}
Write-Host "built $msix"

# --- the side-by-side manifest in lightloomd.exe ----------------------------------
if ($EmbedIn) {
    $mt = Find-SdkTool "mt.exe"
    $merged = Join-Path $OutDir "lightloomd.merged.manifest"
    & $mt -nologo "-inputresource:$EmbedIn;#1" -manifest $sxs "-out:$merged"
    if ($LASTEXITCODE -ne 0) { throw "mt.exe merge failed ($LASTEXITCODE)" }
    & $mt -nologo -manifest $merged "-outputresource:$EmbedIn;#1"
    if ($LASTEXITCODE -ne 0) { throw "mt.exe embed failed ($LASTEXITCODE)" }
    Write-Host "embedded the msix manifest into $EmbedIn (re-sign the exe afterwards if it was signed)"
}

Write-Host ""
Write-Host "Not done by this script (run yourself, per user, not elevated):"
Write-Host "  Add-AppxPackage -Path `"$msix`" -ExternalLocation <directory that holds $Executable>"
Write-Host "  # unsigned dev package:   add -AllowUnsigned"
Write-Host "  # self-signed dev cert:   Import-Certificate -FilePath dev.cer -CertStoreLocation Cert:\CurrentUser\TrustedPeople"
Write-Host "  # undo:                   Get-AppxPackage $Name | Remove-AppxPackage"
Write-Host "Then: Settings > Personalization > Dynamic Lighting > Background light control: put Lightloom first."
