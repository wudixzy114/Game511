param(
    [ValidateSet("html")]
    [string]$Action = "html",
    [string]$LogDir = ".\logs",
    [string]$Output = ".\logs\log-report.html"
)

$ErrorActionPreference = "Stop"

if (-not (Test-Path $LogDir)) {
    New-Item -ItemType Directory -Path $LogDir | Out-Null
}

switch ($Action) {
    "html" {
        cargo run --bin log_report -- --log-dir $LogDir --output $Output
        Write-Host "Log HTML report: $Output"
    }
}
