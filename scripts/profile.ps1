param(
    [ValidateSet("tracy", "flamegraph")]
    [string]$Mode = "tracy",
    [double]$Seconds = 12.0,
    [ValidateSet("presentation", "exploration", "material-gallery")]
    [string]$GameMode = "presentation",
    [string]$Output = ".\\logs\\flamegraph.svg"
)

$ErrorActionPreference = "Stop"

function Ensure-LogDir {
    if (-not (Test-Path ".\\logs")) {
        New-Item -ItemType Directory -Path ".\\logs" | Out-Null
    }
}

Ensure-LogDir

switch ($Mode) {
    "tracy" {
        $env:DAO_AUTO_EXIT_SECONDS = "$Seconds"
        $env:DAO_AUTO_START_MODE = $GameMode
        $env:DAO_PROFILE_TRACY = "1"

        Write-Host "Starting Tracy-enabled capture: mode=$GameMode seconds=$Seconds"
        Write-Host "Open the Tracy desktop app and connect while the game is running."
        cargo run --features tracy-profile --bin dao_game
    }
    "flamegraph" {
        if (-not (Get-Command cargo-flamegraph -ErrorAction SilentlyContinue) -and
            -not (cargo --list | Select-String "flamegraph")) {
            throw "cargo-flamegraph is not installed. Run: cargo install flamegraph"
        }

        $env:DAO_AUTO_EXIT_SECONDS = "$Seconds"
        $env:DAO_AUTO_START_MODE = $GameMode

        Write-Host "Capturing flamegraph: mode=$GameMode seconds=$Seconds output=$Output"
        cargo flamegraph --bin dao_game --output $Output
        Write-Host "Flamegraph written to: $Output"
    }
}
