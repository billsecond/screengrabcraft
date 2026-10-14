<#
.SYNOPSIS
  Build, sign and package ScreenGrabCraft for Windows.

.DESCRIPTION
  Produces, in $env:DIST (default: dist/release):
    screengrabcraft-<version>-windows-<arch>.msi            per-machine installer (WiX v5)
    screengrabcraft-<version>-windows-<arch>-portable.zip   the three programs + docs

  The three programs:
    screengrabcraft.exe          the capture service (tray icon, capture shortcuts; Windows GUI program)
    screengrabcraft-editor.exe   the annotation editor (Windows GUI program)
    screengrabcraft-cli.exe      the command-line program (console program)

  The binaries link the C runtime statically (+crt-static), so neither the MSI nor the portable
  zip needs the Visual C++ redistributable. Signing is delegated to sign.ps1 (skipped with a
  warning when no signing secrets are set).

  Needs: Rust (MSVC toolchain + the target), the Windows SDK (rc.exe, signtool.exe),
  and WiX v5 with its UI and Util extensions (installer dialogs; closing the running capture
  service on upgrade and uninstall):
    dotnet tool install --global wix --version 5.0.2
    wix extension add -g WixToolset.UI.wixext/5.0.2
    wix extension add -g WixToolset.Util.wixext/5.0.2

.EXAMPLE
  pwsh packaging/windows/package.ps1 -Arch x64
  pwsh packaging/windows/package.ps1 -Arch x86 -SkipBuild
  pwsh packaging/windows/package.ps1 -Arch arm64     # cross-compiled; needs the MSVC ARM64 build tools
#>
param(
  [ValidateSet('x64', 'x86', 'arm64')] [string] $Arch = 'x64',
  [switch] $SkipBuild
)
$ErrorActionPreference = 'Stop'
$Root = (Resolve-Path (Join-Path $PSScriptRoot '..\..')).Path

function Invoke-Native([string] $What, [scriptblock] $Block) {
  Write-Output "==> $What"
  & $Block
  if ($LASTEXITCODE -ne 0) { throw "$What failed with exit code $LASTEXITCODE" }
}

# The version lives in one place: [workspace.package] version in the root Cargo.toml.
$Version = $env:SCREENGRABCRAFT_VERSION
if (-not $Version) {
  $inPkg = $false
  foreach ($line in Get-Content (Join-Path $Root 'Cargo.toml')) {
    if ($line -match '^\s*\[') { $inPkg = ($line.Trim() -eq '[workspace.package]'); continue }
    if ($inPkg -and $line -match '^\s*version\s*=\s*"([^"]+)"') { $Version = $Matches[1]; break }
  }
}
if (-not $Version) { throw 'could not read [workspace.package] version from Cargo.toml' }
# MSI ProductVersion is numeric (major.minor.build); pre-release tags are dropped there.
$MsiVersion = ($Version -split '-')[0]

$Target = switch ($Arch) { 'x64' { 'x86_64-pc-windows-msvc' } 'x86' { 'i686-pc-windows-msvc' } 'arm64' { 'aarch64-pc-windows-msvc' } }
$Dist = if ($env:DIST) { $env:DIST } else { Join-Path $Root 'dist\release' }
$TargetDir = if ($env:CARGO_TARGET_DIR) { $env:CARGO_TARGET_DIR } else { Join-Path $Root 'target' }
New-Item -ItemType Directory -Force -Path $Dist | Out-Null

if (-not $env:SCREENGRABCRAFT_BUILD_SHA) { $env:SCREENGRABCRAFT_BUILD_SHA = (git -C $Root rev-parse HEAD 2>$null) }
if (-not $env:SCREENGRABCRAFT_BUILD_DATE) { $env:SCREENGRABCRAFT_BUILD_DATE = (Get-Date).ToUniversalTime().ToString('yyyy-MM-dd') }

Write-Output "ScreenGrabCraft $Version for Windows $Arch ($Target)"

if (-not $SkipBuild) {
  # Static CRT: no VC++ redistributable needed. Scoped to the target so host build scripts and
  # proc-macros are unaffected.
  $flagVar = 'CARGO_TARGET_' + ($Target.ToUpper() -replace '-', '_') + '_RUSTFLAGS'
  [Environment]::SetEnvironmentVariable($flagVar, '-C target-feature=+crt-static')
  # Fail the build (rather than warn) if the icon/VERSIONINFO can't be embedded.
  $env:SCREENGRABCRAFT_REQUIRE_WINRES = '1'
  Invoke-Native "cargo build ($Target)" {
    cargo build --release --locked -p screengrabcraft -p screengrabcraft-editor -p screengrabcraft-cli --target $Target
  }
}

$Bin = Join-Path $TargetDir "$Target\release"

# Check the PE headers before packaging. Machine (COFF header) must match -Arch, so an x64 build
# can never ship labelled arm64. Subsystem (optional header): 2 = Windows GUI, 3 = console. The
# capture service and the editor must be GUI programs (no console window opens with them, at
# sign-in least of all); the CLI must stay a console program so its output reaches the terminal.
function Get-PeHeader([string] $Path) {
  $bytes = [System.IO.File]::ReadAllBytes($Path)
  $pe = [BitConverter]::ToInt32($bytes, 0x3C)
  return @{ Machine = [BitConverter]::ToUInt16($bytes, $pe + 4); Subsystem = [BitConverter]::ToUInt16($bytes, $pe + 0x5C) }
}
$Machine = switch ($Arch) { 'x64' { 0x8664 } 'x86' { 0x14C } 'arm64' { 0xAA64 } }
$Programs = @(@('screengrabcraft.exe', 2), @('screengrabcraft-editor.exe', 2), @('screengrabcraft-cli.exe', 3))
foreach ($check in $Programs) {
  $h = Get-PeHeader (Join-Path $Bin $check[0])
  if ($h.Machine -ne $Machine) { throw "$($check[0]) is for machine 0x$('{0:X}' -f $h.Machine), expected 0x$('{0:X}' -f $Machine) ($Arch)" }
  if ($h.Subsystem -ne $check[1]) { throw "$($check[0]) has PE subsystem $($h.Subsystem), expected $($check[1])" }
  Write-Output "ok $($check[0]): $Arch, PE subsystem $($h.Subsystem)"
}
$Stage = Join-Path $TargetDir "windows-package\$Arch"
Remove-Item -Recurse -Force $Stage -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force -Path $Stage | Out-Null
$Exes = $Programs | ForEach-Object { Join-Path $Stage $_[0] }
foreach ($check in $Programs) { Copy-Item (Join-Path $Bin $check[0]) $Stage }

& (Join-Path $PSScriptRoot 'sign.ps1') @Exes

# ---- MSI ---------------------------------------------------------------------------------------
# The installer's welcome page shows the license: build its RTF from LICENSE-MIT and LICENSE-APACHE
# (and the SIL Open Font License of the fonts the programs embed) so it never drifts from them.
# Hard-wrapped lines are rejoined into paragraphs so the text wraps to the dialog's width.
function ConvertTo-RtfText([string] $Text) {
  $sb = [System.Text.StringBuilder]::new()
  foreach ($ch in $Text.ToCharArray()) {
    $c = [int] $ch
    if ($c -eq 0x5C -or $c -eq 0x7B -or $c -eq 0x7D) { [void] $sb.Append('\').Append($ch) }
    elseif ($c -gt 127) { [void] $sb.Append("\u$(if ($c -gt 32767) { $c - 65536 } else { $c })?") }
    else { [void] $sb.Append($ch) }
  }
  return $sb.ToString()
}
$paragraphs = [System.Collections.Generic.List[string]]::new()
$paragraphs.Add('\b ScreenGrabCraft\b0')
$paragraphs.Add((ConvertTo-RtfText 'ScreenGrabCraft is dual-licensed under the MIT License or the Apache License, Version 2.0, at your option. Both license texts follow. The fonts built into the programs (Inter and JetBrains Mono) are under the SIL Open Font License 1.1, which follows them; every asset is listed in ATTRIBUTION.md at https://github.com/billsecond/screengrabcraft.'))
$LicenseFiles = @((Join-Path $Root 'LICENSE-MIT'), (Join-Path $Root 'LICENSE-APACHE')) +
  @(Get-ChildItem -Path (Join-Path $Root 'assets\fonts\OFL-*.txt') -ErrorAction SilentlyContinue | Sort-Object Name | ForEach-Object { $_.FullName })
foreach ($file in $LicenseFiles) {
  $title = $true  # each file opens with its license's name (or the font's copyright): show it bold
  foreach ($para in ((Get-Content -Raw $file) -split '\r?\n\s*\r?\n')) {
    $joined = (($para -split '\r?\n') | ForEach-Object { $_.Trim() } | Where-Object { $_ }) -join ' '
    if (-not $joined) { continue }
    $paragraphs.Add($(if ($title) { "\b $(ConvertTo-RtfText $joined)\b0" } else { ConvertTo-RtfText $joined }))
    $title = $false
  }
}
$LicenseRtf = Join-Path $Stage 'license.rtf'
$rtf = '{\rtf1\ansi\ansicpg1252\deff0{\fonttbl{\f0\fswiss\fcharset0 Tahoma;}}\viewkind4\uc1\pard\sa120\f0\fs16 ' +
  ($paragraphs -join "\par`r`n") + "\par`r`n}`r`n"
[System.IO.File]::WriteAllText($LicenseRtf, $rtf, [System.Text.Encoding]::ASCII)

$IconDir = Join-Path $Root 'assets\app-icon'
$Msi = Join-Path $Dist "screengrabcraft-$Version-windows-$Arch.msi"
Invoke-Native 'wix build' {
  wix build (Join-Path $PSScriptRoot 'screengrabcraft.wxs') -arch $Arch `
    -ext WixToolset.UI.wixext -ext WixToolset.Util.wixext `
    -d "Version=$MsiVersion" -d "BinDir=$Stage" `
    -d "IconPath=$(Join-Path $IconDir 'screengrabcraft.ico')" `
    -d "EditorIconPath=$(Join-Path $IconDir 'screengrabcraft-editor.ico')" `
    -d "LicenseRtf=$LicenseRtf" `
    -o $Msi
}
# wix writes its debug symbols (.wixpdb) next to the MSI; keep them out of the release assets.
Remove-Item -Force -ErrorAction SilentlyContinue ([IO.Path]::ChangeExtension($Msi, '.wixpdb'))
& (Join-Path $PSScriptRoot 'sign.ps1') $Msi

# ---- portable zip ------------------------------------------------------------------------------
# The three programs and the docs. Registers nothing: the capture service, run from here, offers
# its own "start at sign-in" setting.
$Portable = Join-Path $TargetDir "windows-package\screengrabcraft-$Version-windows-$Arch-portable"
Remove-Item -Recurse -Force $Portable -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force -Path $Portable | Out-Null
Copy-Item $Exes $Portable
foreach ($f in 'README.md', 'LICENSE', 'LICENSE-MIT', 'LICENSE-APACHE', 'ATTRIBUTION.md') {
  $p = Join-Path $Root $f
  if (Test-Path $p) { Copy-Item $p $Portable }
}
# The licences of the fonts the programs embed.
Get-ChildItem -Path (Join-Path $Root 'assets\fonts\OFL-*.txt') -ErrorAction SilentlyContinue | Copy-Item -Destination $Portable
$Zip = Join-Path $Dist "screengrabcraft-$Version-windows-$Arch-portable.zip"
Remove-Item -Force $Zip -ErrorAction SilentlyContinue
Compress-Archive -Path $Portable -DestinationPath $Zip

# Smoke-test the CLI when this machine can run it. An ARM64 build made on an x64 runner can't run
# here; .github/workflows/windows-arm64.yml installs and runs it on ARM64 instead.
$HostArch = [System.Runtime.InteropServices.RuntimeInformation]::OSArchitecture.ToString().ToLowerInvariant()
if ($Arch -ne 'arm64' -or $HostArch -eq 'arm64') {
  Invoke-Native 'screengrabcraft-cli --version' { & (Join-Path $Stage 'screengrabcraft-cli.exe') --version }
} else {
  Write-Output "skipping screengrabcraft-cli --version: an $Arch build doesn't run on this $HostArch machine"
}
Get-Item $Msi, $Zip | Format-Table Name, Length
