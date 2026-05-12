param(
    [string]$Output = ".\logs\performance-report.html",
    [string]$LogDir = ".\logs"
)

$ErrorActionPreference = "Stop"

cargo run --bin perf_report -- html-latest $Output $LogDir
Write-Host "Open report: $Output"
