param(
    [ValidateSet("tracy", "tracy-analyze")]
    [string]$Mode = "tracy",
    [double]$Seconds = 12.0,
    [ValidateSet("presentation", "exploration", "material-gallery")]
    [string]$GameMode = "presentation",
    [string]$TracyDir = "D:\\windows-0.13.1",
    [string]$Trace = "",
    [string]$CsvDir = ".\\logs\\tracy",
    [string]$AnalysisOutput = "",
    [string]$HtmlOutput = "",
    [string]$Address = "127.0.0.1",
    [int]$Port = 8086,
    [int]$Top = 12,
    [int]$TraceKeep = 2,
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

function Remove-OldTracyTraces {
    param(
        [string]$Directory,
        [int]$Keep = 2
    )

    if ($Keep -lt 1) {
        throw "TraceKeep must be at least 1."
    }
    if (-not (Test-Path -LiteralPath $Directory)) {
        return
    }

    $resolvedDir = (Resolve-Path -LiteralPath $Directory).Path.TrimEnd('\', '/')
    $oldTraces = Get-ChildItem -LiteralPath $Directory -Filter "*.tracy" -File |
        Sort-Object LastWriteTimeUtc -Descending |
        Select-Object -Skip $Keep

    foreach ($trace in $oldTraces) {
        $resolvedTrace = (Resolve-Path -LiteralPath $trace.FullName).Path
        if (-not $resolvedTrace.StartsWith($resolvedDir, [StringComparison]::OrdinalIgnoreCase)) {
            throw "Refusing to remove trace outside CsvDir: $resolvedTrace"
        }
        Write-Host "Removing old Tracy trace: $resolvedTrace"
        Remove-Item -LiteralPath $resolvedTrace
    }
}

function Test-PathInsideDirectory {
    param(
        [string]$Path,
        [string]$Directory
    )

    if (-not (Test-Path -LiteralPath $Path) -or -not (Test-Path -LiteralPath $Directory)) {
        return $false
    }

    $resolvedDir = (Resolve-Path -LiteralPath $Directory).Path.TrimEnd('\', '/')
    $resolvedPath = (Resolve-Path -LiteralPath $Path).Path
    return $resolvedPath.StartsWith($resolvedDir, [StringComparison]::OrdinalIgnoreCase)
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

        $capture.WaitForExit()
        $capture.Refresh()
        if ($null -ne $capture.ExitCode -and $capture.ExitCode -ne 0) {
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

    $traceFile = Get-Item -LiteralPath $TracePath -ErrorAction SilentlyContinue
    if (-not $traceFile) {
        throw "Tracy trace was not created: $TracePath"
    }
    if ($traceFile.Length -le 0) {
        throw "Tracy trace was empty: $TracePath"
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
        [string]$ReportPath,
        [string]$HtmlPath
    )

    $analyzerArgs = @(
        "--summary", $Csv.Summary,
        "--self", $Csv.Self,
        "--events", $Csv.Events,
        "--top", "$Top",
        "--html", $HtmlPath
    )

    Write-Host "Running Tracy heuristic analyzer"
    $analysis = & cargo run --quiet --bin tracy_analyze -- @analyzerArgs
    if ($LASTEXITCODE -ne 0) {
        throw "tracy_analyze failed with code $LASTEXITCODE"
    }

    $reportParent = Split-Path -Parent $ReportPath
    if ($reportParent -and -not (Test-Path $reportParent)) {
        New-Item -ItemType Directory -Path $reportParent | Out-Null
    }

    $analysis | Tee-Object -FilePath $ReportPath
    Write-Host "Analysis report: $ReportPath"
    Write-Host "HTML report: $HtmlPath"
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

        if (Test-Path -LiteralPath $tracePath) {
            Write-Host "Using existing Tracy trace: $tracePath"
        } else {
            Invoke-TracyCapture -TracePath $tracePath
            if (Test-PathInsideDirectory -Path $tracePath -Directory $CsvDir) {
                Remove-OldTracyTraces -Directory $CsvDir -Keep $TraceKeep
            }
        }

        $baseName = [IO.Path]::GetFileNameWithoutExtension($tracePath)
        $prefix = Join-Path $CsvDir $baseName
        $csv = Export-TracyCsv -TracePath $tracePath -Prefix $prefix

        $reportPath = if ($AnalysisOutput) {
            $AnalysisOutput
        } else {
            "$prefix-analysis.txt"
        }
        $htmlPath = if ($HtmlOutput) {
            $HtmlOutput
        } else {
            "$prefix-analysis.html"
        }
        Invoke-TracyHeuristicAnalysis -Csv $csv -ReportPath $reportPath -HtmlPath $htmlPath
    }
}
