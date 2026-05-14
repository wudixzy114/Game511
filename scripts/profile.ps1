param(
    [ValidateSet("tracy", "tracy-analyze", "flamegraph")]
    [string]$Mode = "tracy",
    [double]$Seconds = 12.0,
    [ValidateSet("presentation", "exploration", "material-gallery")]
    [string]$GameMode = "presentation",
    [string]$Output = ".\\logs\\flamegraph.svg",
    [string]$TracyDir = "D:\\windows-0.13.1",
    [string]$Trace = "",
    [string]$CsvDir = ".\\logs\\tracy",
    [string]$AnalysisOutput = "",
    [string]$Address = "127.0.0.1",
    [int]$Port = 8086,
    [int]$Top = 12,
    [double]$CaptureTimeoutSeconds = 180.0
)

$ErrorActionPreference = "Stop"

function Ensure-LogDir {
    param(
        [string]$Path = ".\\logs"
    )

    if (-not (Test-Path $Path)) {
        New-Item -ItemType Directory -Path $Path | Out-Null
    }
}

function Resolve-TracyTool {
    param(
        [string]$Name
    )

    $tool = Join-Path $TracyDir $Name
    if (-not (Test-Path $tool)) {
        throw "Tracy tool was not found: $tool"
    }
    return (Resolve-Path $tool).Path
}

function Start-ProfiledGame {
    param(
        [double]$RunSeconds
    )

    $env:DAO_AUTO_EXIT_SECONDS = "$RunSeconds"
    $env:DAO_AUTO_START_MODE = $GameMode
    $env:DAO_PROFILE_TRACY = "1"

    $arguments = @("run", "--features", "tracy-profile", "--bin", "dao_game")
    return Start-Process -FilePath "cargo" -ArgumentList $arguments -PassThru -NoNewWindow
}

function Invoke-TracyCapture {
    param(
        [string]$TracePath
    )

    $captureExe = Resolve-TracyTool "tracy-capture.exe"
    $captureSeconds = [int][Math]::Ceiling($Seconds)
    $gameRunSeconds = [Math]::Ceiling($Seconds + 4.0)
    $timeoutSeconds = [Math]::Ceiling($CaptureTimeoutSeconds + $Seconds)

    $traceParent = Split-Path -Parent $TracePath
    if ($traceParent -and -not (Test-Path $traceParent)) {
        New-Item -ItemType Directory -Path $traceParent | Out-Null
    }

    Write-Host "Starting Tracy-enabled game: mode=$GameMode game_seconds=$gameRunSeconds"
    $game = Start-ProfiledGame -RunSeconds $gameRunSeconds
    try {
        Write-Host "Capturing Tracy trace: seconds=$captureSeconds output=$TracePath"
        $captureArgs = @(
            "-o", $TracePath,
            "-a", $Address,
            "-p", "$Port",
            "-s", "$captureSeconds",
            "-f"
        )
        $capture = Start-Process -FilePath $captureExe -ArgumentList $captureArgs -PassThru -NoNewWindow
        $deadline = (Get-Date).AddSeconds($timeoutSeconds)

        while (-not $capture.HasExited) {
            Start-Sleep -Milliseconds 500
            $capture.Refresh()
            $game.Refresh()

            if ($game.HasExited -and -not $capture.HasExited) {
                Stop-Process -Id $capture.Id -Force -ErrorAction SilentlyContinue
                throw "Game exited before Tracy capture completed. Build may have failed, or Tracy could not connect."
            }

            if ((Get-Date) -gt $deadline) {
                Stop-Process -Id $capture.Id -Force -ErrorAction SilentlyContinue
                if (-not $game.HasExited) {
                    Stop-Process -Id $game.Id -Force -ErrorAction SilentlyContinue
                }
                throw "Timed out waiting for Tracy capture after $timeoutSeconds seconds."
            }
        }

        if ($capture.ExitCode -ne 0) {
            throw "tracy-capture exited with code $($capture.ExitCode)"
        }
    }
    finally {
        $game.Refresh()
        if (-not $game.HasExited) {
            try {
                Wait-Process -Id $game.Id -Timeout 8
            }
            catch {
                Write-Warning "Game process is still running after capture; it should auto-exit soon. Process id: $($game.Id)"
            }
        }
    }

    if (-not (Test-Path $TracePath)) {
        throw "Tracy trace was not created: $TracePath"
    }
}

function Export-TracyCsv {
    param(
        [string]$TracePath,
        [string]$Prefix
    )

    $csvExportExe = Resolve-TracyTool "tracy-csvexport.exe"
    $summaryCsv = "$Prefix-summary.csv"
    $selfCsv = "$Prefix-self.csv"
    $eventsCsv = "$Prefix-events.csv"

    Write-Host "Exporting Tracy summary CSV: $summaryCsv"
    & $csvExportExe -s ";" $TracePath | Set-Content -Path $summaryCsv -Encoding UTF8
    if ($LASTEXITCODE -ne 0) {
        throw "tracy-csvexport summary export failed with code $LASTEXITCODE"
    }

    Write-Host "Exporting Tracy self-time CSV: $selfCsv"
    & $csvExportExe -e -s ";" $TracePath | Set-Content -Path $selfCsv -Encoding UTF8
    if ($LASTEXITCODE -ne 0) {
        throw "tracy-csvexport self-time export failed with code $LASTEXITCODE"
    }

    Write-Host "Exporting Tracy event CSV: $eventsCsv"
    & $csvExportExe -u -s ";" $TracePath | Set-Content -Path $eventsCsv -Encoding UTF8
    if ($LASTEXITCODE -ne 0) {
        throw "tracy-csvexport event export failed with code $LASTEXITCODE"
    }

    return @{
        Summary = $summaryCsv
        Self = $selfCsv
        Events = $eventsCsv
    }
}

function Invoke-TracyHeuristicAnalysis {
    param(
        [hashtable]$Csv,
        [string]$ReportPath
    )

    $analyzerArgs = @(
        "--summary", $Csv.Summary,
        "--self", $Csv.Self,
        "--events", $Csv.Events,
        "--top", "$Top"
    )

    Write-Host "Running Tracy heuristic analyzer"
    $analysis = & cargo run --quiet --bin tracy_analyze -- @analyzerArgs
    if ($LASTEXITCODE -ne 0) {
        throw "tracy_analyze failed with code $LASTEXITCODE"
    }

    $analysis | Tee-Object -FilePath $ReportPath
    Write-Host "Analysis report: $ReportPath"
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
    "tracy-analyze" {
        Ensure-LogDir -Path $CsvDir

        if ($Trace) {
            $tracePath = $Trace
        } else {
            $stamp = Get-Date -Format "yyyyMMdd-HHmmss"
            $tracePath = Join-Path $CsvDir "tracy-$stamp.tracy"
        }

        if (Test-Path $tracePath) {
            Write-Host "Using existing Tracy trace: $tracePath"
        } else {
            Invoke-TracyCapture -TracePath $tracePath
        }

        $baseName = [IO.Path]::GetFileNameWithoutExtension($tracePath)
        $prefix = Join-Path $CsvDir $baseName
        $csv = Export-TracyCsv -TracePath $tracePath -Prefix $prefix

        $reportPath = if ($AnalysisOutput) {
            $AnalysisOutput
        } else {
            "$prefix-analysis.txt"
        }
        Invoke-TracyHeuristicAnalysis -Csv $csv -ReportPath $reportPath
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
