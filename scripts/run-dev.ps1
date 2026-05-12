$ErrorActionPreference = "Stop"

if (-not (Test-Path ".\\logs")) {
    New-Item -ItemType Directory -Path ".\\logs" | Out-Null
}

Write-Host "Starting Dao prototype..."
cargo run --bin dao_game
