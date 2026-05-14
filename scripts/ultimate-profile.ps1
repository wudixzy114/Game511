param(
    [double]$Seconds = 30.0,
    [ValidateSet("presentation", "exploration", "material-gallery")]
    [string]$GameMode = "presentation",
    [string]$OutputDir = ".\\logs\\ultimate",
    [string]$LogDir = ".\\logs",
    [string]$TracyDir = "D:\\windows-0.13.1",
    [string]$TracyCsvDir = ".\\logs\\tracy",
    [int]$Top = 12,
    [int]$TraceKeep = 2,
    [int]$ResultKeep = 2,
    [double]$CaptureTimeoutSeconds = 180.0,
    [switch]$KeepIntermediates
)

$ErrorActionPreference = "Stop"

function Ensure-Dir {
    param([string]$Path)
    if (-not (Test-Path -LiteralPath $Path)) {
        New-Item -ItemType Directory -Path $Path | Out-Null
    }
}

function Html-Escape {
    param([AllowNull()][string]$Value)
    if ($null -eq $Value) {
        return ""
    }
    return $Value.
        Replace("&", "&amp;").
        Replace("<", "&lt;").
        Replace(">", "&gt;").
        Replace('"', "&quot;")
}

function Format-Number {
    param(
        [AllowNull()]$Value,
        [int]$Digits = 2
    )
    if ($null -eq $Value) {
        return "n/a"
    }
    return ([double]$Value).ToString("F$Digits", [Globalization.CultureInfo]::InvariantCulture)
}

function Get-RelativePathText {
    param([string]$Path)
    try {
        return [IO.Path]::GetRelativePath((Get-Location).Path, (Resolve-Path -LiteralPath $Path).Path)
    }
    catch {
        return $Path
    }
}

function Read-TextFile {
    param([string]$Path)
    if (Test-Path -LiteralPath $Path) {
        return Get-Content -Raw -Encoding UTF8 -LiteralPath $Path
    }
    return ""
}

function Remove-GeneratedFile {
    param([AllowNull()][string]$Path)
    if ([string]::IsNullOrWhiteSpace($Path)) {
        return
    }
    if (Test-Path -LiteralPath $Path) {
        Write-Host "Removing intermediate file: $Path"
        Remove-Item -LiteralPath $Path
    }
}

function Remove-IntermediateFiles {
    param([string[]]$Paths)
    if ($KeepIntermediates) {
        return
    }
    foreach ($path in $Paths) {
        Remove-GeneratedFile -Path $path
    }
}

function Remove-OldUltimateResults {
    param(
        [string]$Directory,
        [int]$Keep = 1
    )

    if ($Keep -lt 1) {
        throw "ResultKeep must be at least 1."
    }
    if (-not (Test-Path -LiteralPath $Directory)) {
        return
    }

    foreach ($pattern in @("ultimate-*-ai.md", "ultimate-*-report.html")) {
        $oldFiles = Get-ChildItem -LiteralPath $Directory -Filter $pattern -File |
            Sort-Object LastWriteTimeUtc -Descending |
            Select-Object -Skip $Keep
        foreach ($file in $oldFiles) {
            Write-Host "Removing old ultimate result: $($file.FullName)"
            Remove-Item -LiteralPath $file.FullName
        }
    }
}

function Convert-PhasesToMarkdown {
    param($Phases)
    if (-not $Phases) {
        return "- n/a"
    }
    return ($Phases | Select-Object -First 8 | ForEach-Object {
        "- $($_.phase): avg $(Format-Number $_.average_ms) ms, p95 $(Format-Number $_.p95_ms) ms, p99 $(Format-Number $_.p99_ms) ms, max $(Format-Number $_.max_ms) ms"
    }) -join "`n"
}

function Convert-BottlenecksToMarkdown {
    param($Bottlenecks)
    if (-not $Bottlenecks) {
        return "- n/a"
    }
    return ($Bottlenecks | ForEach-Object {
        "- [$($_.level)] $($_.title): $($_.detail)"
    }) -join "`n"
}

function Convert-ToIndentedBlock {
    param([AllowNull()][string]$Value)
    if ([string]::IsNullOrEmpty($Value)) {
        return "    n/a"
    }
    return (($Value -split "`r?`n") | ForEach-Object { "    $_" }) -join "`n"
}

function Get-PerfBudgetMs {
    param(
        $Perf,
        [string]$PerfText
    )

    if ($Perf.PSObject.Properties.Name -contains "budget_ms" -and $null -ne $Perf.budget_ms) {
        return [double]$Perf.budget_ms
    }
    $match = [regex]::Match($PerfText, "budget_ms:\s*([0-9]+(?:\.[0-9]+)?)")
    if ($match.Success) {
        return [double]::Parse($match.Groups[1].Value, [Globalization.CultureInfo]::InvariantCulture)
    }
    return $null
}

function Convert-PhasesToHtmlRows {
    param($Phases)
    if (-not $Phases) {
        return "<tr><td colspan=""5"">n/a</td></tr>"
    }
    return ($Phases | Select-Object -First 10 | ForEach-Object {
        "<tr><td>$(Html-Escape $_.phase)</td><td class=""num"">$(Format-Number $_.average_ms)</td><td class=""num"">$(Format-Number $_.p95_ms)</td><td class=""num"">$(Format-Number $_.p99_ms)</td><td class=""num"">$(Format-Number $_.max_ms)</td></tr>"
    }) -join "`n"
}

function Convert-BottlenecksToHtml {
    param($Bottlenecks)
    if (-not $Bottlenecks) {
        return "<li>n/a</li>"
    }
    return ($Bottlenecks | ForEach-Object {
        "<li><strong>[$(Html-Escape $_.level)] $(Html-Escape $_.title)</strong><br><span>$(Html-Escape $_.detail)</span></li>"
    }) -join "`n"
}

Ensure-Dir $OutputDir
Ensure-Dir $LogDir
Ensure-Dir $TracyCsvDir

$stamp = Get-Date -Format "yyyyMMdd-HHmmss"
$prefix = Join-Path $OutputDir "ultimate-$stamp"
$tracePath = Join-Path $TracyCsvDir "ultimate-$stamp.tracy"
$tracyTxt = "$prefix-tracy.txt"
$tracyHtml = "$prefix-tracy.html"
$perfTxt = "$prefix-perf.txt"
$perfJsonPath = "$prefix-perf.json"
$perfHtml = "$prefix-perf.html"
$aiReport = "$prefix-ai.md"
$webReport = "$prefix-report.html"
$perfLog = Join-Path $LogDir "performance.log"
$applicationLog = Join-Path $LogDir "application.log"
$errorLog = Join-Path $LogDir "error.log"
$perfLogSnapshot = "$prefix-performance.log"

Write-Host "Ultimate performance run: mode=$GameMode seconds=$Seconds"
Write-Host "Step 1/4: capture Tracy + perf data"
$profileArgs = @(
    "-NoProfile",
    "-ExecutionPolicy", "Bypass",
    "-File", ".\\scripts\\profile.ps1",
    "-Mode", "tracy-analyze",
    "-Seconds", "$Seconds",
    "-GameMode", $GameMode,
    "-TracyDir", $TracyDir,
    "-Trace", $tracePath,
    "-CsvDir", $TracyCsvDir,
    "-AnalysisOutput", $tracyTxt,
    "-HtmlOutput", $tracyHtml,
    "-Top", "$Top",
    "-TraceKeep", "$TraceKeep",
    "-CaptureTimeoutSeconds", "$CaptureTimeoutSeconds"
)
if ($KeepIntermediates) {
    $profileArgs += "-KeepCsv"
}
& powershell @profileArgs
if ($LASTEXITCODE -ne 0) {
    throw "Tracy capture/analyze failed with code $LASTEXITCODE"
}

if (-not (Test-Path -LiteralPath $perfLog)) {
    throw "Performance log was not created: $perfLog"
}
Copy-Item -LiteralPath $perfLog -Destination $perfLogSnapshot -Force

Write-Host "Step 2/4: generate perf reports"
$perfTextOutput = & cargo run --quiet --bin perf_report -- $perfLogSnapshot
if ($LASTEXITCODE -ne 0) {
    throw "perf_report text failed with code $LASTEXITCODE"
}
$perfTextOutput | Set-Content -Encoding UTF8 -Path $perfTxt

$perfJsonOutput = & cargo run --quiet --bin perf_report -- --json $perfLogSnapshot
if ($LASTEXITCODE -ne 0) {
    throw "perf_report json failed with code $LASTEXITCODE"
}
$perfJsonOutput | Set-Content -Encoding UTF8 -Path $perfJsonPath
$perf = $perfJsonOutput | ConvertFrom-Json

& cargo run --quiet --bin perf_report -- html $perfHtml $perfLogSnapshot
if ($LASTEXITCODE -ne 0) {
    throw "perf_report html failed with code $LASTEXITCODE"
}

Write-Host "Step 3/4: build AI summary"
$tracyText = Read-TextFile $tracyTxt
$perfText = Read-TextFile $perfTxt
$budgetMs = Get-PerfBudgetMs -Perf $perf -PerfText $perfText
$overBudgetRate = if ([double]$perf.frames -gt 0) {
    [double]$perf.over_budget_frames * 100.0 / [double]$perf.frames
} else {
    0.0
}
$coverage = if ($perf.frame_detail -and [double]$perf.average_frame_ms -gt 0) {
    [double]$perf.frame_detail.average_profiled_phase_ms * 100.0 / [double]$perf.average_frame_ms
} else {
    0.0
}

$ai = @"
# Ultimate Performance Report

Generated: $(Get-Date -Format "yyyy-MM-dd HH:mm:ss")
Mode: $GameMode
Duration: $Seconds s

## Verdict Inputs

- Current perf log: $(Get-RelativePathText $perfLog)
- Tracy trace: $(Get-RelativePathText $tracePath)
- Human HTML summary: $(Get-RelativePathText $webReport)
- Perf text and Tracy text are embedded below; intermediate subreports are deleted by default.

## Perf Summary

- Frames: $($perf.frames)
- Budget: $(Format-Number $budgetMs) ms
- Average frame: $(Format-Number $perf.average_frame_ms) ms
- P50 / P95 / P99: $(Format-Number $perf.frame_stats.p50) / $(Format-Number $perf.frame_stats.p95) / $(Format-Number $perf.frame_stats.p99) ms
- Worst frame: $(Format-Number $perf.worst_frame_ms) ms
- Over-budget frames: $($perf.over_budget_frames) ($(Format-Number $overBudgetRate 1)%)
- Average profiled phase coverage: $(Format-Number $coverage 1)% of average frame time
- Main schedule avg: $(Format-Number $perf.frame_detail.average_main_schedule_ms) ms
- Render schedule avg: $(Format-Number $perf.frame_detail.average_render_schedule_ms) ms

## Perf Bottlenecks

$(Convert-BottlenecksToMarkdown $perf.bottlenecks)

## Top Perf Phases

$(Convert-PhasesToMarkdown $perf.phases)

## Tracy Heuristic Summary

$(Convert-ToIndentedBlock $tracyText)

## Full Perf Text

$(Convert-ToIndentedBlock $perfText)

## AI Reading Rules

- Use perf data to decide whether the run is actually over budget and which high-level stage is failing.
- Use Tracy data to identify concrete zones/functions that explain CPU work and spikes.
- If perf reports low instrumentation coverage, treat Tracy as the primary hotspot locator and add/adjust project-level instrumentation before making narrow optimization claims.
- Do not claim an optimization is effective until a before/after `perf.ps1` comparison confirms average, p95/p99, and over-budget frames improved.
"@
$ai | Set-Content -Encoding UTF8 -Path $aiReport

Write-Host "Step 4/4: build human HTML summary"
$bottleneckHtml = Convert-BottlenecksToHtml $perf.bottlenecks
$phaseRows = Convert-PhasesToHtmlRows $perf.phases
$tracyTextHtml = Html-Escape $tracyText
$perfTextHtml = Html-Escape $perfText
$aiPathRel = Html-Escape (Get-RelativePathText $aiReport)
$tracePathRel = Html-Escape (Get-RelativePathText $tracePath)
$perfLogRel = Html-Escape (Get-RelativePathText $perfLog)
$web = @"
<!doctype html>
<html lang="zh-CN">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width, initial-scale=1">
  <title>Ultimate Performance Report</title>
  <style>
    :root { --ink:#15201d; --muted:#65736d; --paper:#f7f0df; --card:#fffdf7; --line:#ddcfb7; --good:#0b756d; --warn:#d46b39; --bad:#9f2f2f; --blue:#253f67; }
    * { box-sizing:border-box; }
    body { margin:0; color:var(--ink); background:radial-gradient(circle at 10% 0%, rgba(212,107,57,.18), transparent 28rem), radial-gradient(circle at 90% 10%, rgba(11,117,109,.16), transparent 32rem), linear-gradient(135deg,#f7f0df,#ead9bc); font-family:Georgia, "Times New Roman", serif; }
    main { width:min(1180px, calc(100vw - 28px)); margin:32px auto 56px; }
    .hero { display:grid; grid-template-columns:minmax(0,1.25fr) minmax(280px,.75fr); gap:18px; margin-bottom:18px; }
    .card { background:rgba(255,253,247,.94); border:1px solid var(--line); border-radius:24px; box-shadow:0 18px 55px rgba(44,36,24,.14); padding:24px; }
    h1 { margin:0; font-size:clamp(2.6rem,7vw,6rem); line-height:.86; letter-spacing:-.065em; }
    h2 { margin:0 0 14px; color:var(--muted); font:700 .8rem "Trebuchet MS", Verdana, sans-serif; letter-spacing:.1em; text-transform:uppercase; }
    p, li, td, th, a, .meta { font-family:"Trebuchet MS", Verdana, sans-serif; }
    a { color:var(--good); font-weight:700; }
    .meta { display:grid; gap:9px; color:var(--muted); word-break:break-all; }
    .metrics { display:grid; grid-template-columns:repeat(5, minmax(0,1fr)); gap:12px; margin-bottom:18px; }
    .metric { background:linear-gradient(180deg, rgba(255,255,255,.7), rgba(232,218,191,.34)); border:1px solid var(--line); border-radius:18px; padding:16px; }
    .metric span { display:block; color:var(--muted); font:700 .72rem "Trebuchet MS", Verdana, sans-serif; letter-spacing:.08em; text-transform:uppercase; }
    .metric strong { display:block; margin-top:7px; font-size:1.75rem; }
    .bad strong { color:var(--bad); }
    .warn strong { color:var(--warn); }
    table { width:100%; border-collapse:collapse; font-family:"Trebuchet MS", Verdana, sans-serif; }
    th, td { padding:10px 9px; border-bottom:1px solid var(--line); text-align:left; }
    th { color:var(--muted); font-size:.72rem; letter-spacing:.07em; text-transform:uppercase; }
    .num { text-align:right; white-space:nowrap; }
    ul { margin:0; padding-left:20px; }
    li { margin:0 0 10px; }
    li span { color:var(--muted); }
    pre { margin:0; padding:18px; max-height:520px; overflow:auto; border:1px solid var(--line); border-radius:18px; background:#1f2825; color:#e9f1ed; font:12px Consolas, monospace; }
    .grid { display:grid; grid-template-columns:1fr 1fr; gap:18px; }
    @media (max-width:900px) { .hero,.metrics,.grid { grid-template-columns:1fr; } table { display:block; overflow-x:auto; white-space:nowrap; } main { width:min(100vw - 18px,1180px); margin-top:12px; } }
  </style>
</head>
<body>
  <main>
    <section class="hero">
      <div class="card">
        <h1>Ultimate<br>Performance</h1>
        <p>30-second combined report from perf frame data and Tracy zone data.</p>
      </div>
      <div class="card">
        <h2>Artifacts</h2>
        <div class="meta">
          <div>AI report: <a href="$aiPathRel">$aiPathRel</a></div>
          <div>Tracy trace: <a href="$tracePathRel">$tracePathRel</a></div>
          <div>Perf log: <a href="$perfLogRel">$perfLogRel</a></div>
        </div>
      </div>
    </section>
    <section class="metrics">
      <div class="metric"><span>frames</span><strong>$($perf.frames)</strong></div>
      <div class="metric"><span>budget</span><strong>$(Format-Number $budgetMs) ms</strong></div>
      <div class="metric bad"><span>average frame</span><strong>$(Format-Number $perf.average_frame_ms) ms</strong></div>
      <div class="metric warn"><span>p95 / p99</span><strong>$(Format-Number $perf.frame_stats.p95) / $(Format-Number $perf.frame_stats.p99)</strong></div>
      <div class="metric bad"><span>over budget</span><strong>$(Format-Number $overBudgetRate 1)%</strong></div>
      <div class="metric"><span>coverage</span><strong>$(Format-Number $coverage 1)%</strong></div>
    </section>
    <section class="grid">
      <div class="card">
        <h2>Perf Bottlenecks</h2>
        <ul>$bottleneckHtml</ul>
      </div>
      <div class="card">
        <h2>Stage Signals</h2>
        <p>Main schedule avg: <strong>$(Format-Number $perf.frame_detail.average_main_schedule_ms) ms</strong></p>
        <p>Render schedule avg: <strong>$(Format-Number $perf.frame_detail.average_render_schedule_ms) ms</strong></p>
        <p>Worst frame: <strong>$(Format-Number $perf.worst_frame_ms) ms</strong></p>
      </div>
    </section>
    <section class="card" style="margin-top:18px">
      <h2>Top Perf Phases</h2>
      <table><thead><tr><th>Phase</th><th class="num">Avg</th><th class="num">P95</th><th class="num">P99</th><th class="num">Max</th></tr></thead><tbody>$phaseRows</tbody></table>
    </section>
    <section class="card" style="margin-top:18px">
      <h2>Tracy Heuristic Output</h2>
      <pre>$tracyTextHtml</pre>
    </section>
    <section class="card" style="margin-top:18px">
      <h2>Perf Text Output</h2>
      <pre>$perfTextHtml</pre>
    </section>
  </main>
</body>
</html>
"@
$web | Set-Content -Encoding UTF8 -Path $webReport

Remove-IntermediateFiles -Paths @(
    $tracyTxt,
    $tracyHtml,
    $perfTxt,
    $perfJsonPath,
    $perfHtml,
    $perfLogSnapshot
)
Remove-OldUltimateResults -Directory $OutputDir -Keep $ResultKeep

Write-Host "Ultimate AI report: $aiReport"
Write-Host "Ultimate HTML report: $webReport"
