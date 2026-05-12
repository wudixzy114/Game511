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

function New-PerfSnapshotName([string]$Prefix) {
    $timestamp = Get-Date -Format "yyyyMMdd-HHmmss"
    Join-Path $LogDir "$Prefix-$Mode-$timestamp.log"
}

function Run-GameCapture([string]$SnapshotPath) {
    Ensure-LogDir
    $source = Join-Path $LogDir "performance.log"
    $env:DAO_AUTO_EXIT_SECONDS = "$Seconds"
    $env:DAO_AUTO_START_MODE = $Mode

    Write-Host "Running performance capture: mode=$Mode seconds=$Seconds"
    cargo run --bin dao_game

    if (-not (Test-Path $source)) {
        throw "Performance log was not created: $source"
    }
    Copy-Item $source $SnapshotPath
    Write-Host "Performance snapshot: $SnapshotPath"
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
        $timestamp = Get-Date -Format "yyyyMMdd-HHmmss"
        $autoBaseline = Join-Path $LogDir "performance-baseline-before-$timestamp.log"
        $autoCandidate = Join-Path $LogDir "performance-$Mode-$timestamp.log"

        if (Test-Path $source) {
            Copy-Item $source $autoBaseline
        }

        Run-GameCapture $autoCandidate

        if (Test-Path $autoBaseline) {
            cargo run --bin perf_report -- html $Output $autoBaseline $autoCandidate
        } else {
            cargo run --bin perf_report -- html $Output $autoCandidate
        }
        Write-Host "HTML report: $Output"
    }
    "capture" {
        $snapshot = if ($Candidate) { $Candidate } else { New-PerfSnapshotName "performance" }
        Run-GameCapture $snapshot
        Invoke-PerfReport @($snapshot)
    }
    "report" {
        $target = if ($Log) { $Log } else { Join-Path $LogDir "performance.log" }
        Invoke-PerfReport @($target)
    }
    "compare" {
        if ($Baseline -and $Candidate) {
            Invoke-PerfReport @($Baseline, $Candidate)
        } else {
            Invoke-PerfReport @("compare-latest", $LogDir)
        }
    }
    "html" {
        if ($Baseline -and $Candidate) {
            cargo run --bin perf_report -- html $Output $Baseline $Candidate
        } elseif ($Log) {
            cargo run --bin perf_report -- html $Output $Log
        } else {
            cargo run --bin perf_report -- html-latest $Output $LogDir
        }
        Write-Host "HTML report: $Output"
    }
}
