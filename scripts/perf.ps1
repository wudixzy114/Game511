param(
    [ValidateSet("auto", "capture", "report", "compare", "html")]
    [string]$Action = "auto",
    [double]$Seconds = 12.0,
    [ValidateSet("presentation", "exploration")]
    [string]$Mode = "presentation",
    [string]$Log = "",
    [string]$Baseline = "",
    [string]$Candidate = "",
    [string]$Output = ".\logs\performance-report.html",
    [string]$LogDir = ".\logs",
    [switch]$Json
)

$ErrorActionPreference = "Stop"

function Ensure-LogDir {
    if (-not (Test-Path $LogDir)) {
        New-Item -ItemType Directory -Path $LogDir | Out-Null
    }
}

function Run-GameCapture {
    Ensure-LogDir
    $source = Join-Path $LogDir "performance.log"
    $env:DAO_AUTO_EXIT_SECONDS = "$Seconds"
    $env:DAO_AUTO_START_MODE = $Mode

    Write-Host "Running performance capture: mode=$Mode seconds=$Seconds"
    cargo run --bin dao_game

    if (-not (Test-Path $source)) {
        throw "Performance log was not created: $source"
    }
}

function Invoke-PerfReport([string[]]$Args) {
    if ($Json) {
        cargo run --bin perf_report -- --json @Args
    } else {
        cargo run --bin perf_report -- @Args
    }
}

Ensure-LogDir

switch ($Action) {
    "auto" {
        $source = Join-Path $LogDir "performance.log"
        $baseline = Join-Path $LogDir "performance.log.1"

        Run-GameCapture

        if (Test-Path $baseline) {
            cargo run --bin perf_report -- html $Output $baseline $source
        } else {
            cargo run --bin perf_report -- html $Output $source
        }
        Write-Host "HTML report: $Output"
    }
    "capture" {
        Run-GameCapture
        Invoke-PerfReport @((Join-Path $LogDir "performance.log"))
    }
    "report" {
        $target = if ($Log) { $Log } else { Join-Path $LogDir "performance.log" }
        Invoke-PerfReport @($target)
    }
    "compare" {
        if ($Baseline -and $Candidate) {
            Invoke-PerfReport @($Baseline, $Candidate)
        } else {
            $baseline = Join-Path $LogDir "performance.log.1"
            $candidate = Join-Path $LogDir "performance.log"
            if (-not (Test-Path $baseline)) {
                throw "No previous performance log found: $baseline"
            }
            Invoke-PerfReport @($baseline, $candidate)
        }
    }
    "html" {
        if ($Baseline -and $Candidate) {
            cargo run --bin perf_report -- html $Output $Baseline $Candidate
        } elseif ($Log) {
            cargo run --bin perf_report -- html $Output $Log
        } else {
            $baseline = Join-Path $LogDir "performance.log.1"
            $candidate = Join-Path $LogDir "performance.log"
            if (Test-Path $baseline) {
                cargo run --bin perf_report -- html $Output $baseline $candidate
            } else {
                cargo run --bin perf_report -- html $Output $candidate
            }
        }
        Write-Host "HTML report: $Output"
    }
}
