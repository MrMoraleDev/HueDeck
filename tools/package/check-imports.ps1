# SPDX-License-Identifier: GPL-3.0-or-later
<#
.SYNOPSIS
  Fail when a Lightloom binary imports a DLL that a clean Windows does not have.

.DESCRIPTION
  Reads the PE import table and the delay-load import table of each file and checks every
  imported DLL name against an allow-list of DLLs that ship with Windows 10 and 11. A DLL that
  sits next to the checked binary in the same directory (libcef.dll, chrome_elf.dll) also
  passes, because it is shipped with it. The C/C++ runtime never passes, even when present:
  VCRUNTIME140*.dll, MSVCP*.dll, the UCRT forwarders api-ms-win-crt-*.dll, ucrtbase.dll.
  Lightloom links the CRT statically (.cargo/config.toml, `+crt-static`), so any of them
  means a binary was built without it.

  Only pass Lightloom's own binaries. CEF's DLLs are prebuilt upstream and not checked.

.PARAMETER Path
  Files, or directories whose *.exe files are checked (not recursive).

.EXAMPLE
  tools/package/check-imports.ps1 dist/Lightloom
.EXAMPLE
  tools/package/check-imports.ps1 target/release/lightloomd.exe target/release/lightloom.exe
#>
param(
  [Parameter(Mandatory = $true, Position = 0, ValueFromRemainingArguments = $true)] [string[]] $Path
)
$ErrorActionPreference = "Stop"

# System DLLs present on every supported Windows (10 and 11, x64). Wildcards allowed.
$allowed = @(
  "kernel32.dll", "kernelbase.dll", "ntdll.dll", "user32.dll", "gdi32.dll", "advapi32.dll",
  "ws2_32.dll", "bcrypt.dll", "bcryptprimitives.dll", "ncrypt.dll", "crypt32.dll", "secur32.dll",
  "ole32.dll", "oleaut32.dll", "combase.dll", "shell32.dll", "shlwapi.dll", "shcore.dll",
  "setupapi.dll", "cfgmgr32.dll", "hid.dll", "winusb.dll", "iphlpapi.dll", "userenv.dll",
  "dbghelp.dll", "psapi.dll", "powrprof.dll", "version.dll", "winmm.dll", "comctl32.dll",
  "propsys.dll", "dwmapi.dll", "uxtheme.dll", "imm32.dll", "netapi32.dll", "wtsapi32.dll",
  "mswsock.dll", "dnsapi.dll", "rpcrt4.dll", "comdlg32.dll", "winhttp.dll", "dxgi.dll",
  "d3d11.dll", "d3d12.dll", "d3dcompiler_47.dll", "mfplat.dll", "mf.dll", "mfreadwrite.dll",
  "mmdevapi.dll", "avrt.dll", "wevtapi.dll", "taskschd.dll", "wintrust.dll", "msimg32.dll",
  "api-ms-win-core-*.dll", "api-ms-win-security-*.dll", "api-ms-win-shcore-*.dll",
  "api-ms-win-eventing-*.dll", "api-ms-win-power-*.dll", "api-ms-win-devices-*.dll",
  "api-ms-win-service-*.dll"
)
# The C/C++ runtime: never allowed, even if a copy sits next to the binary.
$forbidden = @("vcruntime*.dll", "msvcp*.dll", "msvcr*.dll", "ucrtbase*.dll", "api-ms-win-crt-*.dll",
  "concrt*.dll", "vccorlib*.dll")

function Read-Imports([string] $File) {
  $b = [System.IO.File]::ReadAllBytes($File)
  if ($b.Length -lt 0x40 -or [BitConverter]::ToUInt16($b, 0) -ne 0x5A4D) { throw "$File is not a PE file" }
  $pe = [BitConverter]::ToInt32($b, 0x3C)
  if ([BitConverter]::ToUInt32($b, $pe) -ne 0x4550) { throw "$File has no PE signature" }
  $coff = $pe + 4
  $nsec = [BitConverter]::ToUInt16($b, $coff + 2)
  $optSize = [BitConverter]::ToUInt16($b, $coff + 16)
  $opt = $coff + 20
  $dirs = switch ([BitConverter]::ToUInt16($b, $opt)) {
    0x20b { $opt + 112 }  # PE32+
    0x10b { $opt + 96 }   # PE32
    default { throw "$File has an unknown optional header" }
  }
  $sections = for ($i = 0; $i -lt $nsec; $i++) {
    $s = $opt + $optSize + 40 * $i
    [pscustomobject]@{
      Va = [BitConverter]::ToUInt32($b, $s + 12)
      Size = [Math]::Max([BitConverter]::ToUInt32($b, $s + 8), [BitConverter]::ToUInt32($b, $s + 16))
      Raw = [BitConverter]::ToUInt32($b, $s + 20)
    }
  }
  $toOffset = {
    param([uint32] $rva)
    foreach ($s in $sections) {
      if ($rva -ge $s.Va -and $rva -lt $s.Va + $s.Size) { return [int64]($rva - $s.Va + $s.Raw) }
    }
    throw "$File`: RVA 0x{0:X} is outside every section" -f $rva
  }
  $readName = {
    param([uint32] $rva)
    $o = & $toOffset $rva
    $e = $o
    while ($b[$e] -ne 0) { $e++ }
    [System.Text.Encoding]::ASCII.GetString($b, $o, $e - $o)
  }
  # Data directory 1: imports (20-byte descriptors, name RVA at +12).
  # Data directory 13: delay-load imports (32-byte descriptors, name RVA at +4).
  foreach ($d in @(@{ Index = 1; Stride = 20; NameAt = 12 }, @{ Index = 13; Stride = 32; NameAt = 4 })) {
    $rva = [BitConverter]::ToUInt32($b, $dirs + 8 * $d.Index)
    if ($rva -eq 0) { continue }
    $o = & $toOffset $rva
    while ($true) {
      $name = [BitConverter]::ToUInt32($b, $o + $d.NameAt)
      if ($name -eq 0) { break }
      & $readName $name
      $o += $d.Stride
    }
  }
}

$files = foreach ($p in $Path) {
  if (Test-Path -PathType Container $p) { Get-ChildItem -File -Filter "*.exe" $p }
  elseif (Test-Path -PathType Leaf $p) { Get-Item $p }
  else { throw "missing: $p" }
}
if (-not $files) { throw "no binaries to check in: $($Path -join ', ')" }

$bad = 0
foreach ($f in $files) {
  $imports = @(Read-Imports $f.FullName | Sort-Object -Unique)
  $rejected = foreach ($dll in $imports) {
    $lower = $dll.ToLowerInvariant()
    if ($forbidden | Where-Object { $lower -like $_ }) { "$dll (C runtime)"; continue }
    if ($allowed | Where-Object { $lower -like $_ }) { continue }
    if (Test-Path -PathType Leaf (Join-Path $f.DirectoryName $dll)) { continue }
    "$dll (not a system DLL and not shipped next to it)"
  }
  if ($rejected) {
    $bad++
    Write-Host "FAIL $($f.Name): $($imports -join ', ')"
    Write-Host "     rejected: $($rejected -join '; ')"
  } else {
    Write-Host "ok   $($f.Name): $($imports -join ', ')"
  }
}
if ($bad) { throw "check-imports: $bad binar$(if ($bad -eq 1) { 'y' } else { 'ies' }) import DLLs a clean Windows lacks" }
