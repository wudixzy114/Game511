$ErrorActionPreference = "Stop"

Write-Host "Running cargo fmt --check..."
cargo fmt --all --check

Write-Host "Running cargo check..."
cargo check

Write-Host "Running cargo test..."
cargo test

Write-Host "Running cargo clippy..."
cargo clippy --all-targets -- -D warnings
