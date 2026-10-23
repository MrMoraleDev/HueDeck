# SPDX-License-Identifier: GPL-3.0-or-later
<#
.SYNOPSIS
  Write the winget manifests of a HueDeck release from the templates in template/.

.DESCRIPTION
  Fills ${VERSION}, ${INSTALLER_URL}, ${INSTALLER_SHA256} and ${PRODUCT_CODE} (read from the
  MSI's Property table) and writes manifests/h/HueDeck/HueDeck/<version>/. Run it on the signed
  MSI that the release publishes, then validate (`winget validate <dir>`) and open the pull
  request against microsoft/winget-pkgs (for example with `wingetcreate submit <dir>`).

.EXAMPLE
  packaging/winget/render.ps1 -Msi dist/HueDeck-0.1.0-x64.msi `
    -Url https://github.com/MrMoraleDev/HueDeck/releases/download/v0.1.0/HueDeck-0.1.0-x64.msi
#>
param(
  [Parameter(Mandatory = $true)] [string] $Msi,
  [Parameter(Mandatory = $true)] [string] $Url,
  [string] $Out = ""
)
$ErrorActionPreference = "Stop"
$Msi = (Resolve-Path $Msi).Path

function Get-MsiProperty([string] $Path, [string] $Name) {
  $installer = New-Object -ComObject WindowsInstaller.Installer
  $db = $installer.GetType().InvokeMember("OpenDatabase", "InvokeMethod", $null, $installer, @($Path, 0))
  $view = $db.GetType().InvokeMember("OpenView", "InvokeMethod", $null, $db,
    @("SELECT ``Value`` FROM ``Property`` WHERE ``Property`` = '$Name'"))
  [void] $view.GetType().InvokeMember("Execute", "InvokeMethod", $null, $view, $null)
  $record = $view.GetType().InvokeMember("Fetch", "InvokeMethod", $null, $view, $null)
  if (-not $record) { throw "$Path has no $Name" }
  $record.GetType().InvokeMember("StringData", "GetProperty", $null, $record, 1)
}

$values = @{
  VERSION          = Get-MsiProperty $Msi "ProductVersion"
  PRODUCT_CODE     = Get-MsiProperty $Msi "ProductCode"
  INSTALLER_URL    = $Url
  INSTALLER_SHA256 = (Get-FileHash $Msi -Algorithm SHA256).Hash
}
if (-not $Out) { $Out = Join-Path $PSScriptRoot "manifests/h/HueDeck/HueDeck/$($values.VERSION)" }
New-Item -ItemType Directory -Force $Out | Out-Null
foreach ($t in Get-ChildItem (Join-Path $PSScriptRoot "template") -Filter "*.yaml") {
  $text = Get-Content $t.FullName -Raw
  foreach ($k in $values.Keys) { $text = $text.Replace('${' + $k + '}', $values[$k]) }
  # The template header is for this repository, not for winget-pkgs.
  $text = ($text -split "`n" | Where-Object { $_ -notmatch '^# (SPDX|Template:)' }) -join "`n"
  if ($text -match '\$\{[A-Z_]+\}') { throw "$($t.Name): unfilled $($Matches[0])" }
  [System.IO.File]::WriteAllText((Join-Path $Out $t.Name), ($text -replace "`r`n", "`n"))
}
"wrote $Out"
