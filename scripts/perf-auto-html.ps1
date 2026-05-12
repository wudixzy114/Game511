param(
    [double]$Seconds = 12.0,
    [ValidateSet("presentation", "exploration")]
    [string]$Mode = "presentation",
    [string]$Output = ".\logs\performance-report.html"
)

$ErrorActionPreference = "Stop"

if (-not (Test-Path ".\logs")) {
    New-Item -ItemType Directory -Path ".\logs" | Out-Null
}

$timestamp = Get-Date -Format "yyyyMMdd-HHmmss"
$source = ".\logs\performance.log"
$baseline = ".\logs\performance-baseline-before-$timestamp.log"
$candidate = ".\logs\performance-$Mode-$timestamp.log"

if (Test-Path $source) {
    Copy-Item $source $baseline
}

$env:DAO_AUTO_EXIT_SECONDS = "$Seconds"
$env:DAO_AUTO_START_MODE = $Mode

Write-Host "Running unattended performance capture: mode=$Mode seconds=$Seconds"
cargo run --bin dao_game

if (-not (Test-Path $source)) {
    throw "Performance log was not created: $source"
}

Copy-Item $source $candidate

if (Test-Path $baseline) {
    cargo run --bin perf_report -- html $Output $baseline $candidate
} else {
    cargo run --bin perf_report -- html $Output $candidate
}

Write-Host "Performance snapshot: $candidate"
Write-Host "HTML report: $Output"
