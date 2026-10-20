<#
.SYNOPSIS
  Keep Cargo's build cache from eating the disk.

.DESCRIPTION
  `target/` is generated, not the product. Debug builds of this workspace once
  reached 25 GB: PDB files, duplicate rlibs and incremental sessions left by
  repeated Tauri test builds. This script reports the size and cleans the debug
  profile when it is over the limit, or everything with -All.

  It refuses to run while a build or the app is running, because deleting
  artifacts under a live cargo only fails the build for no good reason.

.PARAMETER LimitGB
  Clean target/debug when it is larger than this. Default 3.

.PARAMETER All
  Run `cargo clean`: remove target/ entirely. Copy a release exe out first.

.PARAMETER Force
  Clean even when cargo, rustc or nvd-app is running. Do not use it mid-build.
#>
[CmdletBinding()]
param(
    [double]$LimitGB = 3,
    [switch]$All,
    [switch]$Force
)

$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $PSScriptRoot
$target = Join-Path $root "target"

function Get-SizeGB([string]$Path) {
    if (-not (Test-Path -LiteralPath $Path)) { return 0.0 }
    $bytes = (Get-ChildItem -LiteralPath $Path -Recurse -File -Force -ErrorAction SilentlyContinue |
        Measure-Object -Property Length -Sum).Sum
    if ($null -eq $bytes) { return 0.0 }
    return [math]::Round($bytes / 1GB, 2)
}

if (-not (Test-Path -LiteralPath $target)) {
    Write-Host "target/ is not there; nothing to clean."
    exit 0
}

# The cleanup is only a backstop; the profile is what keeps a fresh build small.
# Fail the gate if someone removes it, rather than silently letting the cache
# grow again.
$manifest = Get-Content -LiteralPath (Join-Path $root "Cargo.toml") -Raw
foreach ($needle in @('[profile.dev]', 'debug = 1', 'incremental = false', '[profile.dev.package."*"]')) {
    if ($manifest -notmatch [regex]::Escape($needle)) {
        Write-Warning "Cargo.toml lost the build-cache profile ($needle); see docs/engineering/testing.md#the-build-cache."
        exit 1
    }
}

$busy = @(Get-Process -Name cargo, rustc, nvd-app -ErrorAction SilentlyContinue)
if ($busy.Count -gt 0 -and -not $Force) {
    $names = ($busy | ForEach-Object ProcessName) -join ", "
    Write-Warning "A build or the app is running ($names); not cleaning. Close it, or pass -Force."
    exit 1
}

$before = Get-SizeGB $target
$debugBefore = Get-SizeGB (Join-Path $target "debug")
if (-not $All -and $debugBefore -le $LimitGB) {
    Write-Host ("target/debug is {0} GB (limit {1} GB), target/ is {2} GB; leaving it alone." -f $debugBefore, $LimitGB, $before)
    exit 0
}

$release = Join-Path $target "release\nvd-app.exe"
if ($All -and (Test-Path -LiteralPath $release)) {
    Write-Warning "A release build is at $release. Copy it out before -All if you need it; this removes it."
}

Push-Location $root
try {
    if ($All) {
        Write-Host ("Cleaning all of target/ ({0} GB)..." -f $before)
        cargo clean
    } else {
        Write-Host ("Cleaning target/debug ({0} GB, limit {1} GB; target/ is {2} GB)..." -f $debugBefore, $LimitGB, $before)
        cargo clean --profile dev
    }
    if ($LASTEXITCODE -ne 0) { throw "cargo clean failed with exit code $LASTEXITCODE" }
} finally {
    Pop-Location
}

Write-Host ("target/ is now {0} GB." -f (Get-SizeGB $target))
