$ErrorActionPreference = "Stop"

if (-not (Test-Path ".\\logs")) {
    New-Item -ItemType Directory -Path ".\\logs" | Out-Null
}

$env:DAO_PRESENTATION_MODE = "1"

Write-Host "Starting Dao presentation scene..."
cargo run
