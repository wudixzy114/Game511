$ErrorActionPreference = "Stop"

if (-not (Test-Path ".\logs")) {
    New-Item -ItemType Directory -Path ".\logs" | Out-Null
}

$env:DAO_AUTO_START_MODE = "material-gallery"

Write-Host "Starting Dao material gallery..."
cargo run --bin dao_game
