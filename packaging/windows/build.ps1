# Build the Windows artifacts: a standalone zip and a per-user Inno Setup
# installer for realitytrace.
#
# Usage: packaging/windows/build.ps1 <version> <bindir> <outdir>
#
# Both outputs are UNSIGNED (no Authenticode certificate is used).
# Pattern adapted from DomainForge packaging/windows/build.ps1.
param(
    [Parameter(Mandatory = $true, Position = 0)][string]$Version,
    [Parameter(Mandatory = $true, Position = 1)][string]$BinDir,
    [Parameter(Mandatory = $true, Position = 2)][string]$OutDir
)

$ErrorActionPreference = 'Stop'
if ($Version -notmatch '^[0-9]+\.[0-9]+\.[0-9]+([-+][0-9A-Za-z.-]+)?$') {
    throw "invalid version '$Version'"
}

$repo = (Resolve-Path (Join-Path $PSScriptRoot '..\..')).Path
$bin = (Resolve-Path $BinDir).Path
New-Item -ItemType Directory -Force -Path $OutDir | Out-Null
$out = (Resolve-Path $OutDir).Path
$license = Join-Path $repo 'LICENSE'
$base = "realitytrace-$Version-x86_64-pc-windows-msvc"
$exe = Join-Path $bin 'realitytrace.exe'

foreach ($file in @($exe, $license)) {
    if (-not (Test-Path $file)) { throw "missing required file $file" }
}

# Standalone zip: the executable and licence at the archive root.
$zip = Join-Path $out "$base.zip"
Remove-Item -Force -ErrorAction SilentlyContinue $zip
$stage = Join-Path ([System.IO.Path]::GetTempPath()) ("realitytrace-zip-" + [guid]::NewGuid())
New-Item -ItemType Directory -Force -Path $stage | Out-Null
Copy-Item $exe (Join-Path $stage 'realitytrace.exe')
Copy-Item $license (Join-Path $stage 'LICENSE')
Compress-Archive -Path (Join-Path $stage '*') -DestinationPath $zip
Remove-Item -Recurse -Force $stage

# Installer: locate Inno Setup 6 (ISCC.exe).
$iscc = $null
$cmd = Get-Command iscc.exe -ErrorAction SilentlyContinue
if ($cmd) { $iscc = $cmd.Source }
if (-not $iscc) {
    $candidate = Join-Path ${env:ProgramFiles(x86)} 'Inno Setup 6\ISCC.exe'
    if (Test-Path $candidate) { $iscc = $candidate }
}
if (-not $iscc) { throw 'ISCC.exe not found; install Inno Setup 6 (choco install innosetup)' }

& $iscc `
    "/DAppVersion=$Version" `
    "/DBinDir=$bin" `
    "/DLicenseFile=$license" `
    "/DOutDir=$out" `
    "/DSetupBase=$base-setup" `
    (Join-Path $PSScriptRoot 'realitytrace.iss')
if ($LASTEXITCODE -ne 0) { throw "ISCC failed with exit code $LASTEXITCODE" }

$setup = Join-Path $out "$base-setup.exe"
if (-not (Test-Path $setup)) { throw "installer not produced: $setup" }

Get-ChildItem -Path $out | Select-Object Name, Length |
    Format-Table -AutoSize | Out-String | Write-Host
