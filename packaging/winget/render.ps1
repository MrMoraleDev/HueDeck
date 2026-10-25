# SPDX-License-Identifier: GPL-3.0-or-later
<#
.SYNOPSIS
  Write the winget manifests of a Lightloom release from the templates in template/.

.DESCRIPTION
  Fills ${VERSION}, ${INSTALLER_URL}, ${INSTALLER_SHA256} and ${PRODUCT_CODE} and writes
  manifests/l/Lightloom/Lightloom/<version>/ (the path the manifests take in
  microsoft/winget-pkgs). Submission steps: packaging/winget/README.md.

  -Tag (the usual way): downloads the release's SHA256SUMS and Lightloom-<x.y.z>-x64.msi from
  GitHub, checks the MSI against SHA256SUMS, and reads ProductVersion and ProductCode from the
  MSI's Property table. The ProductCode changes with every build (WiX generates it), so it
  must come from the MSI the release publishes, never from a local build.

  -Msi with -Url: the same from an MSI already on disk; -Sha256Sums checks it against the
  release's SHA256SUMS as well.

  Reading the MSI needs Windows (the WindowsInstaller.Installer COM object).

.EXAMPLE
  packaging/winget/render.ps1 -Tag v0.1.0

.EXAMPLE
  packaging/winget/render.ps1 -Msi dist/Lightloom-0.1.0-x64.msi -Sha256Sums dist/SHA256SUMS `
    -Url https://github.com/MrMoraleDev/Lightloom/releases/download/v0.1.0/Lightloom-0.1.0-x64.msi
#>
[CmdletBinding(DefaultParameterSetName = "Tag")]
param(
  [Parameter(Mandatory = $true, ParameterSetName = "Tag")] [string] $Tag,
  # Where the release files are; the default is the GitHub release of -Tag.
  [Parameter(ParameterSetName = "Tag")] [string] $BaseUrl = "",
  [Parameter(Mandatory = $true, ParameterSetName = "Msi")] [string] $Msi,
  [Parameter(Mandatory = $true, ParameterSetName = "Msi")] [string] $Url,
  [Parameter(ParameterSetName = "Msi")] [string] $Sha256Sums = "",
  [string] $Out = ""
)
$ErrorActionPreference = "Stop"
$ProgressPreference = "SilentlyContinue"

function Get-MsiProperty([string] $Path, [string] $Name) {
  $installer = New-Object -ComObject WindowsInstaller.Installer
  $db = $installer.GetType().InvokeMember("OpenDatabase", "InvokeMethod", $null, $installer, @($Path, 0))
  $view = $db.GetType().InvokeMember("OpenView", "InvokeMethod", $null, $db,
    @("SELECT ``Value`` FROM ``Property`` WHERE ``Property`` = '$Name'"))
  [void] $view.GetType().InvokeMember("Execute", "InvokeMethod", $null, $view, $null)
  $record = $view.GetType().InvokeMember("Fetch", "InvokeMethod", $null, $view, $null)
  if (-not $record) { throw "$Path has no $Name" }
  $value = $record.GetType().InvokeMember("StringData", "GetProperty", $null, $record, 1)
  # Release the database, so the temporary MSI can be deleted afterwards.
  [void] $view.GetType().InvokeMember("Close", "InvokeMethod", $null, $view, $null)
  [void] [System.Runtime.InteropServices.Marshal]::ReleaseComObject($record)
  [void] [System.Runtime.InteropServices.Marshal]::ReleaseComObject($view)
  [void] [System.Runtime.InteropServices.Marshal]::ReleaseComObject($db)
  $value
}

# The SHA-256 SHA256SUMS lists for $Name (sha256sum format: "<hash>  <name>" or "<hash> *<name>").
function Get-ListedHash([string] $SumsText, [string] $Name) {
  foreach ($line in ($SumsText -split "`r?`n")) {
    if ($line -match '^([0-9a-fA-F]{64}) [ *](.+)$' -and $Matches[2] -eq $Name) {
      return $Matches[1].ToUpperInvariant()
    }
  }
  throw "SHA256SUMS has no entry for $Name"
}

$tmp = $null
try {
  if ($PSCmdlet.ParameterSetName -eq "Tag") {
    if ($Tag -notmatch '^v(\d+\.\d+\.\d+)(-.+)?$') { throw "not a release tag: $Tag (vX.Y.Z)" }
    # The MSI is named after the numeric version (release-build.yml `meta`: msi_version).
    $msiName = "Lightloom-$($Matches[1])-x64.msi"
    if (-not $BaseUrl) { $BaseUrl = "https://github.com/MrMoraleDev/Lightloom/releases/download/$Tag" }
    $BaseUrl = $BaseUrl.TrimEnd("/")
    $tmp = Join-Path ([System.IO.Path]::GetTempPath()) ("lightloom-winget-" + [guid]::NewGuid())
    New-Item -ItemType Directory $tmp | Out-Null
    $Sha256Sums = Join-Path $tmp "SHA256SUMS"
    $Msi = Join-Path $tmp $msiName
    $Url = "$BaseUrl/$msiName"
    Invoke-WebRequest -UseBasicParsing -Uri "$BaseUrl/SHA256SUMS" -OutFile $Sha256Sums
    Invoke-WebRequest -UseBasicParsing -Uri $Url -OutFile $Msi
  }
  $Msi = (Resolve-Path $Msi).Path
  $hash = (Get-FileHash $Msi -Algorithm SHA256).Hash.ToUpperInvariant()
  if ($Sha256Sums) {
    $listed = Get-ListedHash (Get-Content $Sha256Sums -Raw) (Split-Path $Url -Leaf)
    if ($listed -ne $hash) { throw "$Msi does not match SHA256SUMS ($hash, listed $listed)" }
  }

  $values = @{
    VERSION          = Get-MsiProperty $Msi "ProductVersion"
    PRODUCT_CODE     = Get-MsiProperty $Msi "ProductCode"
    INSTALLER_URL    = $Url
    INSTALLER_SHA256 = $hash
  }
  if ($values.PRODUCT_CODE -notmatch '^\{[0-9A-Fa-f-]{36}\}$') {
    throw "unexpected ProductCode $($values.PRODUCT_CODE)"
  }
  if ($PSCmdlet.ParameterSetName -eq "Tag" -and $Tag -notmatch "^v$([regex]::Escape($values.VERSION))(-|$)") {
    throw "$Tag does not match the MSI's ProductVersion $($values.VERSION)"
  }

  if (-not $Out) { $Out = Join-Path $PSScriptRoot "manifests/l/Lightloom/Lightloom/$($values.VERSION)" }
  New-Item -ItemType Directory -Force $Out | Out-Null
  foreach ($t in Get-ChildItem (Join-Path $PSScriptRoot "template") -Filter "*.yaml") {
    $text = Get-Content $t.FullName -Raw
    foreach ($k in $values.Keys) { $text = $text.Replace('${' + $k + '}', $values[$k]) }
    # The template header is for this repository, not for winget-pkgs.
    $text = ($text -split "`r?`n" | Where-Object { $_ -notmatch '^# (SPDX|Template:)' }) -join "`n"
    if ($text -match '\$\{[A-Z_]+\}') { throw "$($t.Name): unfilled $($Matches[0])" }
    # UTF-8 without a BOM, LF line endings.
    [System.IO.File]::WriteAllText((Join-Path $Out $t.Name), $text, (New-Object System.Text.UTF8Encoding $false))
  }
  "wrote $Out (version $($values.VERSION), ProductCode $($values.PRODUCT_CODE))"
} finally {
  [GC]::Collect()
  [GC]::WaitForPendingFinalizers()
  if ($tmp -and (Test-Path $tmp)) { Remove-Item -Recurse -Force $tmp }
}
