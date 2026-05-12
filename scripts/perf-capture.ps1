param(
    [double]$Seconds = 8.0,
    [ValidateSet("presentation", "exploration")]
    [string]$Mode = "presentation"
)

$ErrorActionPreference = "Stop"

if (-not (Test-Path ".\logs")) {
    New-Item -ItemType Directory -Path ".\logs" | Out-Null
}

$timestamp = Get-Date -Format "yyyyMMdd-HHmmss"
$source = ".\logs\performance.log"
$snapshot = ".\logs\performance.$timestamp.log"

if (Test-Path $source) {
    Copy-Item $source $snapshot
}

$env:DAO_AUTO_EXIT_SECONDS = "$Seconds"
$env:DAO_AUTO_START_MODE = $Mode
Write-Host "Capturing performance sample for $Seconds seconds in $Mode mode..."
cargo run --bin dao_game

if (Test-Path $source) {
    Copy-Item $source $snapshot
    Write-Host "Performance snapshot: $snapshot"
    cargo run --bin perf_report -- $snapshot
}
