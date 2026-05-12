param(
    [string]$LogDir = ".\logs"
)

$ErrorActionPreference = "Stop"

cargo run --bin perf_report -- compare-latest $LogDir
