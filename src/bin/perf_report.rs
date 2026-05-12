use std::{
    collections::HashMap,
    env, fs,
    fs::File,
    io::{BufRead, BufReader},
    path::{Path, PathBuf},
};

use serde::Deserialize;
use serde_json::{Map, Value};

const DEFAULT_LOG_DIR: &str = "logs";
const DEFAULT_PERF_LOG_NAME: &str = "performance.log";
const DETAIL_PHASES: [&str; 9] = [
    "environment",
    "presentation",
    "player",
    "signs",
    "ui",
    "world_collision",
    "world_impostor",
    "world_streaming",
    "world_visibility",
];
const HTML_CHART_POINTS: usize = 360;

fn main() -> Result<(), String> {
    let args: Vec<String> = env::args().skip(1).collect();
    let command = Command::parse(&args)?;
    match command {
        Command::Single { path, format } => {
            let report = load_report(&path)?;
            print_single_report(&path.display().to_string(), &report, format);
            Ok(())
        }
        Command::Compare {
            baseline,
            candidate,
            format,
        } => {
            let baseline_report = load_report(&baseline)?;
            let candidate_report = load_report(&candidate)?;
            print_comparison_report(
                &baseline.display().to_string(),
                &baseline_report,
                &candidate.display().to_string(),
                &candidate_report,
                format,
            );
            Ok(())
        }
        Command::Latest { log_dir, format } => {
            let path = latest_log_path(&log_dir)?;
            let report = load_report(&path)?;
            print_single_report(&path.display().to_string(), &report, format);
            Ok(())
        }
        Command::CompareLatest { log_dir, format } => {
            let (baseline, candidate) = latest_two_log_paths(&log_dir)?;
            let baseline_report = load_report(&baseline)?;
            let candidate_report = load_report(&candidate)?;
            print_comparison_report(
                &baseline.display().to_string(),
                &baseline_report,
                &candidate.display().to_string(),
                &candidate_report,
                format,
            );
            Ok(())
        }
        Command::Html {
            output,
            baseline,
            candidate,
        } => {
            let baseline_report = load_report(&baseline)?;
            let candidate_report = if let Some(candidate) = &candidate {
                Some((candidate.clone(), load_report(candidate)?))
            } else {
                None
            };
            let html = render_html_report(
                &baseline.display().to_string(),
                &baseline_report,
                candidate_report
                    .as_ref()
                    .map(|(path, report)| (path.display().to_string(), report)),
            )?;
            if let Some(parent) = output
                .parent()
                .filter(|parent| !parent.as_os_str().is_empty())
            {
                fs::create_dir_all(parent)
                    .map_err(|error| format!("failed to create {}: {error}", parent.display()))?;
            }
            fs::write(&output, html)
                .map_err(|error| format!("failed to write {}: {error}", output.display()))?;
            println!("HTML report written: {}", output.display());
            Ok(())
        }
        Command::Help => {
            print_usage();
            Ok(())
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum OutputFormat {
    Text,
    Json,
}

#[derive(Debug, Clone)]
enum Command {
    Single {
        path: PathBuf,
        format: OutputFormat,
    },
    Compare {
        baseline: PathBuf,
        candidate: PathBuf,
        format: OutputFormat,
    },
    Latest {
        log_dir: PathBuf,
        format: OutputFormat,
    },
    CompareLatest {
        log_dir: PathBuf,
        format: OutputFormat,
    },
    Html {
        output: PathBuf,
        baseline: PathBuf,
        candidate: Option<PathBuf>,
    },
    Help,
}

impl Command {
    fn parse(args: &[String]) -> Result<Self, String> {
        if args.is_empty() {
            return Err(usage_error());
        }

        if args[0] == "-h" || args[0] == "--help" || args[0] == "help" {
            return Ok(Self::Help);
        }

        let (format, rest) = parse_format(args)?;
        match rest.as_slice() {
            [command] if command == "latest" => Ok(Self::Latest {
                log_dir: PathBuf::from(DEFAULT_LOG_DIR),
                format,
            }),
            [command, log_dir] if command == "latest" => Ok(Self::Latest {
                log_dir: PathBuf::from(log_dir),
                format,
            }),
            [command] if command == "compare-latest" => Ok(Self::CompareLatest {
                log_dir: PathBuf::from(DEFAULT_LOG_DIR),
                format,
            }),
            [command, log_dir] if command == "compare-latest" => Ok(Self::CompareLatest {
                log_dir: PathBuf::from(log_dir),
                format,
            }),
            [command, output, baseline] if command == "html" => Ok(Self::Html {
                output: PathBuf::from(output),
                baseline: PathBuf::from(baseline),
                candidate: None,
            }),
            [command, output, baseline, candidate] if command == "html" => Ok(Self::Html {
                output: PathBuf::from(output),
                baseline: PathBuf::from(baseline),
                candidate: Some(PathBuf::from(candidate)),
            }),
            [command, output] if command == "html-latest" => {
                let (baseline, candidate) = latest_two_log_paths(Path::new(DEFAULT_LOG_DIR))?;
                Ok(Self::Html {
                    output: PathBuf::from(output),
                    baseline,
                    candidate: Some(candidate),
                })
            }
            [command, output, log_dir] if command == "html-latest" => {
                let (baseline, candidate) = latest_two_log_paths(Path::new(log_dir))?;
                Ok(Self::Html {
                    output: PathBuf::from(output),
                    baseline,
                    candidate: Some(candidate),
                })
            }
            [path] => Ok(Self::Single {
                path: PathBuf::from(path),
                format,
            }),
            [baseline, candidate] => Ok(Self::Compare {
                baseline: PathBuf::from(baseline),
                candidate: PathBuf::from(candidate),
                format,
            }),
            _ => Err(usage_error()),
        }
    }
}

fn parse_format(args: &[String]) -> Result<(OutputFormat, Vec<String>), String> {
    let mut format = OutputFormat::Text;
    let mut rest = Vec::new();
    for arg in args {
        match arg.as_str() {
            "--json" => format = OutputFormat::Json,
            "--text" => format = OutputFormat::Text,
            value if value.starts_with("--") => {
                return Err(format!("unknown option: {value}\n{}", usage_error()));
            }
            _ => rest.push(arg.clone()),
        }
    }
    Ok((format, rest))
}

#[derive(Debug, Default, Clone)]
struct PerfReport {
    session_id: Option<String>,
    frames: u64,
    over_budget_frames: u64,
    worst_frame_ms: f32,
    average_frame_ms: f32,
    average_over_budget_frame_ms: f32,
    sampled_over_budget_total_ms: f32,
    budget_ms: Option<f32>,
    target_fps: Option<f32>,
    frame_samples: Vec<FrameSample>,
    phase_totals: HashMap<String, PhaseSummary>,
    ignored_lines: u64,
}

#[derive(Debug, Default, Clone)]
struct FrameSample {
    frame: u64,
    frame_ms: f32,
    average_ms: Option<f32>,
    budget_delta_ms: Option<f32>,
    profiled_phase_ms: f32,
    phases: HashMap<String, f32>,
}

#[derive(Debug, Default, Clone, Copy)]
struct PhaseSummary {
    total_ms: f32,
    max_ms: f32,
    samples: u64,
}

#[derive(Debug, Clone, Copy)]
struct MetricStats {
    min: f32,
    p50: f32,
    p90: f32,
    p95: f32,
    p99: f32,
    max: f32,
    average: f32,
    stddev: f32,
}

#[derive(Debug, Clone, Copy)]
struct FrameDetailSummary {
    start_frame: u64,
    end_frame: u64,
    average_profiled_phase_ms: f32,
    latest_moving_average_ms: Option<f32>,
    max_budget_delta_ms: Option<f32>,
}

#[derive(Debug, Clone)]
struct PhaseStats {
    phase: String,
    average_ms: f32,
    p95_ms: f32,
    p99_ms: f32,
    max_ms: f32,
    samples: u64,
}

#[derive(Debug, Clone)]
struct BottleneckFinding {
    level: &'static str,
    title: String,
    detail: String,
}

#[derive(Debug, Deserialize)]
struct PerfLogLine {
    target: String,
    #[serde(default)]
    fields: Value,
}

fn load_report(path: &Path) -> Result<PerfReport, String> {
    let file =
        File::open(path).map_err(|error| format!("failed to open {}: {error}", path.display()))?;
    let reader = BufReader::new(file);
    let mut report = PerfReport::default();

    let mut active_session_id: Option<String> = None;

    for line in reader.lines() {
        let line = line.map_err(|error| format!("failed to read {}: {error}", path.display()))?;
        let line = line.trim_start_matches('\u{feff}');
        if line.trim().is_empty() {
            continue;
        }
        let parsed: PerfLogLine = match serde_json::from_str(line) {
            Ok(parsed) => parsed,
            Err(_) => {
                report.ignored_lines += 1;
                continue;
            }
        };
        let Some(fields) = parsed.fields.as_object() else {
            report.ignored_lines += 1;
            continue;
        };
        let line_session_id = parse_string_field(fields, "session_id");
        if parsed.target == "dao_game::performance::session_start" {
            let ignored_lines = report.ignored_lines;
            active_session_id = line_session_id;
            report = PerfReport::default();
            report.session_id = active_session_id.clone();
            report.ignored_lines = ignored_lines;
            report.budget_ms = parse_f32_field(fields, "budget_ms");
            report.target_fps = parse_f32_field(fields, "target_fps");
            continue;
        }
        if active_session_id.is_some() && line_session_id != active_session_id {
            continue;
        }
        match parsed.target.as_str() {
            "dao_game::performance::frame_detail" => {
                if let Some(frame_ms) = parse_f32_field(fields, "frame_ms") {
                    let mut sample = FrameSample {
                        frame: parse_u64_field(fields, "frame").unwrap_or(0),
                        frame_ms,
                        average_ms: parse_f32_field(fields, "average_ms"),
                        budget_delta_ms: parse_f32_field(fields, "budget_delta_ms"),
                        profiled_phase_ms: parse_f32_field(fields, "profiled_phase_ms")
                            .unwrap_or(0.0),
                        phases: HashMap::new(),
                    };
                    report.budget_ms = parse_f32_field(fields, "budget_ms").or(report.budget_ms);
                    for phase in DETAIL_PHASES {
                        let key = format!("{phase}_ms");
                        if let Some(phase_ms) = parse_f32_field(fields, &key) {
                            sample.phases.insert(phase.to_string(), phase_ms);
                        }
                    }
                    report.frame_samples.push(sample);
                }
            }
            "dao_game::performance::frame" => {
                if let Some(frame_ms) = parse_f32_field(fields, "frame_ms") {
                    report.frames += 1;
                    report.worst_frame_ms = report.worst_frame_ms.max(frame_ms);
                    report.average_frame_ms += frame_ms;
                }
            }
            "dao_game::performance::budget" => {
                if let Some(frame_ms) = parse_f32_field(fields, "frame_ms") {
                    report.frames += 1;
                    report.over_budget_frames += 1;
                    report.worst_frame_ms = report.worst_frame_ms.max(frame_ms);
                    report.sampled_over_budget_total_ms += frame_ms;
                    report.budget_ms = parse_f32_field(fields, "budget_ms").or(report.budget_ms);
                    for (name_key, ms_key) in [
                        ("top_phase_1_name", "top_phase_1_ms"),
                        ("top_phase_2_name", "top_phase_2_ms"),
                        ("top_phase_3_name", "top_phase_3_ms"),
                        ("top_phase_4_name", "top_phase_4_ms"),
                        ("top_phase_5_name", "top_phase_5_ms"),
                    ] {
                        if let (Some(name), Some(phase_ms)) = (
                            parse_string_field(fields, name_key),
                            parse_f32_field(fields, ms_key),
                        ) {
                            let summary = report.phase_totals.entry(name).or_default();
                            summary.total_ms += phase_ms;
                            summary.max_ms = summary.max_ms.max(phase_ms);
                            summary.samples += 1;
                        }
                    }
                }
            }
            "dao_game::performance::session" => {
                report.session_id = line_session_id.or(report.session_id);
                report.frames = parse_u64_field(fields, "frames").unwrap_or(report.frames);
                report.over_budget_frames = parse_u64_field(fields, "over_budget_frames")
                    .unwrap_or(report.over_budget_frames);
                report.budget_ms = parse_f32_field(fields, "budget_ms").or(report.budget_ms);
                report.worst_frame_ms =
                    parse_f32_field(fields, "worst_frame_ms").unwrap_or(report.worst_frame_ms);
                report.average_frame_ms =
                    parse_f32_field(fields, "average_frame_ms").unwrap_or(report.average_frame_ms);
                report.average_over_budget_frame_ms =
                    parse_f32_field(fields, "average_over_budget_frame_ms")
                        .unwrap_or(report.average_over_budget_frame_ms);
                report.phase_totals.clear();
                for (name_key, avg_key, max_key) in [
                    (
                        "hot_phase_1_name",
                        "hot_phase_1_avg_ms",
                        "hot_phase_1_max_ms",
                    ),
                    (
                        "hot_phase_2_name",
                        "hot_phase_2_avg_ms",
                        "hot_phase_2_max_ms",
                    ),
                    (
                        "hot_phase_3_name",
                        "hot_phase_3_avg_ms",
                        "hot_phase_3_max_ms",
                    ),
                    (
                        "hot_phase_4_name",
                        "hot_phase_4_avg_ms",
                        "hot_phase_4_max_ms",
                    ),
                    (
                        "hot_phase_5_name",
                        "hot_phase_5_avg_ms",
                        "hot_phase_5_max_ms",
                    ),
                ] {
                    if let (Some(phase), Some(avg_ms), Some(max_ms)) = (
                        parse_string_field(fields, name_key),
                        parse_f32_field(fields, avg_key),
                        parse_f32_field(fields, max_key),
                    ) {
                        report.phase_totals.insert(
                            phase,
                            PhaseSummary {
                                total_ms: avg_ms * report.frames.max(1) as f32,
                                max_ms,
                                samples: report.frames.max(1),
                            },
                        );
                    }
                }
            }
            _ => {}
        }
    }

    finalize_report(&mut report);
    Ok(report)
}

fn finalize_report(report: &mut PerfReport) {
    if !report.frame_samples.is_empty() {
        report.frames = report.frame_samples.len() as u64;
        let values: Vec<f32> = report
            .frame_samples
            .iter()
            .map(|sample| sample.frame_ms)
            .collect();
        let stats = metric_stats(&values);
        report.worst_frame_ms = stats.max;
        report.average_frame_ms = stats.average;
        if let Some(budget_ms) = report.budget_ms {
            report.over_budget_frames = report
                .frame_samples
                .iter()
                .filter(|sample| sample.frame_ms > budget_ms)
                .count() as u64;
            report.average_over_budget_frame_ms =
                average_over_budget_frame_ms(&report.frame_samples, budget_ms);
        }
        report.phase_totals = phase_totals_from_samples(&report.frame_samples);
    } else if report.frames > 0 && report.average_frame_ms > report.worst_frame_ms {
        report.average_frame_ms /= report.frames as f32;
    }

    if report.over_budget_frames > 0 && report.average_over_budget_frame_ms == 0.0 {
        report.average_over_budget_frame_ms =
            report.sampled_over_budget_total_ms / report.over_budget_frames as f32;
    }
}

fn phase_totals_from_samples(samples: &[FrameSample]) -> HashMap<String, PhaseSummary> {
    let mut totals = HashMap::new();
    for sample in samples {
        for (phase, phase_ms) in &sample.phases {
            let summary = totals.entry(phase.clone()).or_insert(PhaseSummary {
                total_ms: 0.0,
                max_ms: 0.0,
                samples: 0,
            });
            summary.total_ms += *phase_ms;
            summary.max_ms = summary.max_ms.max(*phase_ms);
            summary.samples += 1;
        }
    }
    totals
}

fn average_over_budget_frame_ms(samples: &[FrameSample], budget_ms: f32) -> f32 {
    let mut total = 0.0;
    let mut count = 0;
    for sample in samples {
        if sample.frame_ms > budget_ms {
            total += sample.frame_ms;
            count += 1;
        }
    }
    if count == 0 {
        0.0
    } else {
        total / count as f32
    }
}

fn parse_string_field(fields: &Map<String, Value>, key: &str) -> Option<String> {
    match fields.get(key) {
        Some(Value::String(value)) => Some(value.clone()),
        Some(Value::Number(value)) => Some(value.to_string()),
        _ => None,
    }
}

fn parse_f32_field(fields: &Map<String, Value>, key: &str) -> Option<f32> {
    match fields.get(key) {
        Some(Value::Number(value)) => value.as_f64().map(|value| value as f32),
        Some(Value::String(value)) => value.parse::<f32>().ok(),
        _ => None,
    }
}

fn parse_u64_field(fields: &Map<String, Value>, key: &str) -> Option<u64> {
    match fields.get(key) {
        Some(Value::Number(value)) => value.as_u64(),
        Some(Value::String(value)) => value.parse::<u64>().ok(),
        _ => None,
    }
}

fn print_single_report(path: &str, report: &PerfReport, format: OutputFormat) {
    match format {
        OutputFormat::Text => print_single_text_report(path, report),
        OutputFormat::Json => println!("{}", single_report_json(path, report)),
    }
}

fn print_single_text_report(path: &str, report: &PerfReport) {
    println!("Report: {path}");
    if let Some(session_id) = &report.session_id {
        println!("session_id: {session_id}");
    }
    if let Some(budget_ms) = report.budget_ms {
        println!("budget_ms: {:.2}", budget_ms);
    }
    println!("frames: {}", report.frames);
    println!("over_budget_frames: {}", report.over_budget_frames);
    println!("worst_frame_ms: {:.2}", report.worst_frame_ms);
    println!("average_frame_ms: {:.2}", report.average_frame_ms);
    println!(
        "average_over_budget_frame_ms: {:.2}",
        report.average_over_budget_frame_ms
    );
    if let Some(stats) = frame_stats(report) {
        println!(
            "frame_distribution: min {:.2}, p50 {:.2}, p90 {:.2}, p95 {:.2}, p99 {:.2}, stddev {:.2}",
            stats.min, stats.p50, stats.p90, stats.p95, stats.p99, stats.stddev
        );
    }
    if !report.frame_samples.is_empty() {
        println!("detail_samples: {}", report.frame_samples.len());
        if let Some(detail) = frame_detail_summary(report) {
            println!(
                "detail_frame_range: {}..{}",
                detail.start_frame, detail.end_frame
            );
            println!(
                "average_profiled_phase_ms: {:.2}",
                detail.average_profiled_phase_ms
            );
            if let Some(latest_moving_average_ms) = detail.latest_moving_average_ms {
                println!("latest_moving_average_ms: {:.2}", latest_moving_average_ms);
            }
            if let Some(max_budget_delta_ms) = detail.max_budget_delta_ms {
                println!("max_budget_delta_ms: {:+.2}", max_budget_delta_ms);
            }
        }
    }
    if report.ignored_lines > 0 {
        println!("ignored_lines: {}", report.ignored_lines);
    }
    println!("bottlenecks:");
    for finding in bottleneck_findings(report) {
        println!(
            "  [{}] {}: {}",
            finding.level, finding.title, finding.detail
        );
    }
    println!("top_phases:");
    for phase in phase_stats(report) {
        println!(
            "  {}: avg {:.2} ms, p95 {:.2} ms, p99 {:.2} ms, max {:.2} ms",
            phase.phase, phase.average_ms, phase.p95_ms, phase.p99_ms, phase.max_ms
        );
    }
}

fn print_comparison_report(
    baseline_name: &str,
    baseline: &PerfReport,
    candidate_name: &str,
    candidate: &PerfReport,
    format: OutputFormat,
) {
    match format {
        OutputFormat::Text => {
            print_comparison_text_report(baseline_name, baseline, candidate_name, candidate);
        }
        OutputFormat::Json => {
            println!(
                "{}",
                comparison_report_json(baseline_name, baseline, candidate_name, candidate)
            );
        }
    }
}

fn print_comparison_text_report(
    baseline_name: &str,
    baseline: &PerfReport,
    candidate_name: &str,
    candidate: &PerfReport,
) {
    println!("Baseline: {baseline_name}");
    println!("Candidate: {candidate_name}");
    println!(
        "frames: {} -> {} ({:+})",
        baseline.frames,
        candidate.frames,
        candidate.frames as i64 - baseline.frames as i64
    );
    println!(
        "worst_frame_ms: {:.2} -> {:.2} ({:+.2}, {:+.1}%)",
        baseline.worst_frame_ms,
        candidate.worst_frame_ms,
        candidate.worst_frame_ms - baseline.worst_frame_ms,
        percent_delta(baseline.worst_frame_ms, candidate.worst_frame_ms)
    );
    println!(
        "average_frame_ms: {:.2} -> {:.2} ({:+.2}, {:+.1}%)",
        baseline.average_frame_ms,
        candidate.average_frame_ms,
        candidate.average_frame_ms - baseline.average_frame_ms,
        percent_delta(baseline.average_frame_ms, candidate.average_frame_ms)
    );
    println!(
        "over_budget_frames: {} -> {} ({:+})",
        baseline.over_budget_frames,
        candidate.over_budget_frames,
        candidate.over_budget_frames as i64 - baseline.over_budget_frames as i64
    );
    println!(
        "average_over_budget_frame_ms: {:.2} -> {:.2} ({:+.2}, {:+.1}%)",
        baseline.average_over_budget_frame_ms,
        candidate.average_over_budget_frame_ms,
        candidate.average_over_budget_frame_ms - baseline.average_over_budget_frame_ms,
        percent_delta(
            baseline.average_over_budget_frame_ms,
            candidate.average_over_budget_frame_ms
        )
    );
    if let (Some(left), Some(right)) = (frame_stats(baseline), frame_stats(candidate)) {
        println!(
            "p95_frame_ms: {:.2} -> {:.2} ({:+.2}, {:+.1}%)",
            left.p95,
            right.p95,
            right.p95 - left.p95,
            percent_delta(left.p95, right.p95)
        );
        println!(
            "p99_frame_ms: {:.2} -> {:.2} ({:+.2}, {:+.1}%)",
            left.p99,
            right.p99,
            right.p99 - left.p99,
            percent_delta(left.p99, right.p99)
        );
        println!(
            "frame_stddev_ms: {:.2} -> {:.2} ({:+.2}, {:+.1}%)",
            left.stddev,
            right.stddev,
            right.stddev - left.stddev,
            percent_delta(left.stddev, right.stddev)
        );
    }

    println!("candidate_bottlenecks:");
    for finding in bottleneck_findings(candidate) {
        println!(
            "  [{}] {}: {}",
            finding.level, finding.title, finding.detail
        );
    }

    let mut deltas = phase_deltas(baseline, candidate);
    deltas.sort_by(|left, right| right.average_delta.total_cmp(&left.average_delta));
    println!("phase_deltas:");
    for delta in deltas {
        println!(
            "  {}: avg {:.2} -> {:.2} ms ({:+.2}, {:+.1}%), p95 {:.2} -> {:.2} ms ({:+.2})",
            delta.phase,
            delta.baseline_average,
            delta.candidate_average,
            delta.average_delta,
            percent_delta(delta.baseline_average, delta.candidate_average),
            delta.baseline_p95,
            delta.candidate_p95,
            delta.candidate_p95 - delta.baseline_p95
        );
    }
}

#[derive(Debug, Clone)]
struct PhaseDelta {
    phase: String,
    baseline_average: f32,
    candidate_average: f32,
    average_delta: f32,
    baseline_p95: f32,
    candidate_p95: f32,
}

fn phase_deltas(baseline: &PerfReport, candidate: &PerfReport) -> Vec<PhaseDelta> {
    let baseline_stats = phase_stats_by_name(baseline);
    let candidate_stats = phase_stats_by_name(candidate);
    let mut phase_names: Vec<String> = baseline_stats
        .keys()
        .chain(candidate_stats.keys())
        .cloned()
        .collect();
    phase_names.sort();
    phase_names.dedup();

    phase_names
        .into_iter()
        .map(|phase| {
            let baseline = baseline_stats.get(&phase);
            let candidate = candidate_stats.get(&phase);
            let baseline_average = baseline.map(|stats| stats.average_ms).unwrap_or(0.0);
            let candidate_average = candidate.map(|stats| stats.average_ms).unwrap_or(0.0);
            PhaseDelta {
                phase,
                baseline_average,
                candidate_average,
                average_delta: candidate_average - baseline_average,
                baseline_p95: baseline.map(|stats| stats.p95_ms).unwrap_or(0.0),
                candidate_p95: candidate.map(|stats| stats.p95_ms).unwrap_or(0.0),
            }
        })
        .collect()
}

fn frame_stats(report: &PerfReport) -> Option<MetricStats> {
    if report.frame_samples.is_empty() {
        return None;
    }
    let values: Vec<f32> = report
        .frame_samples
        .iter()
        .map(|sample| sample.frame_ms)
        .collect();
    Some(metric_stats(&values))
}

fn frame_detail_summary(report: &PerfReport) -> Option<FrameDetailSummary> {
    let first = report.frame_samples.first()?;
    let last = report.frame_samples.last()?;
    let average_profiled_phase_ms = report
        .frame_samples
        .iter()
        .map(|sample| sample.profiled_phase_ms)
        .sum::<f32>()
        / report.frame_samples.len() as f32;
    let latest_moving_average_ms = report
        .frame_samples
        .iter()
        .rev()
        .find_map(|sample| sample.average_ms);
    let max_budget_delta_ms = report
        .frame_samples
        .iter()
        .filter_map(|sample| sample.budget_delta_ms)
        .max_by(|left, right| left.total_cmp(right));
    Some(FrameDetailSummary {
        start_frame: first.frame,
        end_frame: last.frame,
        average_profiled_phase_ms,
        latest_moving_average_ms,
        max_budget_delta_ms,
    })
}

fn bottleneck_findings(report: &PerfReport) -> Vec<BottleneckFinding> {
    let mut findings = Vec::new();
    let frame_stats = frame_stats(report);
    let phase_stats = phase_stats(report);
    let budget_ms = report.budget_ms;

    if report.frame_samples.is_empty() {
        findings.push(BottleneckFinding {
            level: "warn",
            title: "missing frame detail".to_string(),
            detail: "no frame_detail samples; only coarse session or slow-frame logs are available"
                .to_string(),
        });
    }

    if let (Some(stats), Some(budget_ms)) = (frame_stats, budget_ms) {
        let over_budget_ratio = if report.frames > 0 {
            report.over_budget_frames as f32 / report.frames as f32
        } else {
            0.0
        };
        if stats.p95 > budget_ms {
            findings.push(BottleneckFinding {
                level: "critical",
                title: "frame budget exceeded".to_string(),
                detail: format!(
                    "p95 frame time {:.2} ms is above {:.2} ms budget; {:.1}% frames are over budget",
                    stats.p95,
                    budget_ms,
                    over_budget_ratio * 100.0
                ),
            });
        } else if stats.p95 > budget_ms * 0.85 {
            findings.push(BottleneckFinding {
                level: "warn",
                title: "frame budget is tight".to_string(),
                detail: format!(
                    "p95 frame time {:.2} ms is within 15% of {:.2} ms budget",
                    stats.p95, budget_ms
                ),
            });
        }

        if stats.p99 - stats.p50 > budget_ms * 0.5 {
            findings.push(BottleneckFinding {
                level: "warn",
                title: "frame spikes detected".to_string(),
                detail: format!(
                    "p99-p50 gap is {:.2} ms; inspect spike-heavy phases before average-only tuning",
                    stats.p99 - stats.p50
                ),
            });
        }
    }

    if let Some(top_average) = phase_stats.first() {
        let total_profiled_average = phase_stats
            .iter()
            .map(|phase| phase.average_ms.max(0.0))
            .sum::<f32>();
        let share = if total_profiled_average > f32::EPSILON {
            top_average.average_ms / total_profiled_average
        } else {
            0.0
        };
        findings.push(BottleneckFinding {
            level: if share >= 0.45 { "critical" } else { "info" },
            title: format!("primary average hotspot: {}", top_average.phase),
            detail: format!(
                "avg {:.2} ms, p95 {:.2} ms, max {:.2} ms; {:.1}% of profiled phase time",
                top_average.average_ms,
                top_average.p95_ms,
                top_average.max_ms,
                share * 100.0
            ),
        });
    }

    if let Some(top_spike) = phase_stats
        .iter()
        .max_by(|left, right| left.p99_ms.total_cmp(&right.p99_ms))
        && top_spike.p99_ms > top_spike.average_ms * 2.0
        && top_spike.p99_ms > 1.0
    {
        findings.push(BottleneckFinding {
            level: "warn",
            title: format!("spiky phase: {}", top_spike.phase),
            detail: format!(
                "p99 {:.2} ms is much higher than avg {:.2} ms; look for intermittent work or cache misses",
                top_spike.p99_ms, top_spike.average_ms
            ),
        });
    }

    if let (Some(detail), Some(stats)) = (frame_detail_summary(report), frame_stats) {
        let coverage = if stats.average > f32::EPSILON {
            detail.average_profiled_phase_ms / stats.average
        } else {
            0.0
        };
        if coverage < 0.35 {
            findings.push(BottleneckFinding {
                level: "warn",
                title: "low instrumentation coverage".to_string(),
                detail: format!(
                    "profiled phases explain {:.1}% of average frame time; add phase markers around render prep, asset work, or startup systems",
                    coverage * 100.0
                ),
            });
        }
    }

    if findings.is_empty() {
        findings.push(BottleneckFinding {
            level: "info",
            title: "no obvious bottleneck".to_string(),
            detail: "frame distribution and profiled phases are inside current thresholds"
                .to_string(),
        });
    }

    findings
}

fn phase_stats(report: &PerfReport) -> Vec<PhaseStats> {
    if !report.frame_samples.is_empty() {
        let mut phase_values: HashMap<String, Vec<f32>> = HashMap::new();
        for sample in &report.frame_samples {
            for (phase, value) in &sample.phases {
                phase_values.entry(phase.clone()).or_default().push(*value);
            }
        }
        let mut stats: Vec<PhaseStats> = phase_values
            .into_iter()
            .map(|(phase, values)| {
                let stats = metric_stats(&values);
                PhaseStats {
                    phase,
                    average_ms: stats.average,
                    p95_ms: stats.p95,
                    p99_ms: stats.p99,
                    max_ms: stats.max,
                    samples: values.len() as u64,
                }
            })
            .collect();
        stats.sort_by(|left, right| right.average_ms.total_cmp(&left.average_ms));
        return stats;
    }

    sorted_phases(report)
        .into_iter()
        .map(|(phase, summary)| PhaseStats {
            phase: phase.to_string(),
            average_ms: if report.frames > 0 {
                summary.total_ms / report.frames as f32
            } else {
                0.0
            },
            p95_ms: 0.0,
            p99_ms: 0.0,
            max_ms: summary.max_ms,
            samples: summary.samples,
        })
        .collect()
}

fn phase_stats_by_name(report: &PerfReport) -> HashMap<String, PhaseStats> {
    phase_stats(report)
        .into_iter()
        .map(|stats| (stats.phase.clone(), stats))
        .collect()
}

fn metric_stats(values: &[f32]) -> MetricStats {
    if values.is_empty() {
        return MetricStats {
            min: 0.0,
            p50: 0.0,
            p90: 0.0,
            p95: 0.0,
            p99: 0.0,
            max: 0.0,
            average: 0.0,
            stddev: 0.0,
        };
    }

    let mut sorted = values.to_vec();
    sorted.sort_by(|left, right| left.total_cmp(right));
    let total = sorted.iter().sum::<f32>();
    let average = total / sorted.len() as f32;
    let variance = sorted
        .iter()
        .map(|value| {
            let delta = *value - average;
            delta * delta
        })
        .sum::<f32>()
        / sorted.len() as f32;

    MetricStats {
        min: *sorted.first().unwrap_or(&0.0),
        p50: percentile(&sorted, 0.50),
        p90: percentile(&sorted, 0.90),
        p95: percentile(&sorted, 0.95),
        p99: percentile(&sorted, 0.99),
        max: *sorted.last().unwrap_or(&0.0),
        average,
        stddev: variance.sqrt(),
    }
}

fn percentile(sorted_values: &[f32], percentile: f32) -> f32 {
    if sorted_values.is_empty() {
        return 0.0;
    }
    if sorted_values.len() == 1 {
        return sorted_values[0];
    }
    let position = percentile.clamp(0.0, 1.0) * (sorted_values.len() - 1) as f32;
    let lower = position.floor() as usize;
    let upper = position.ceil() as usize;
    if lower == upper {
        sorted_values[lower]
    } else {
        let fraction = position - lower as f32;
        sorted_values[lower] * (1.0 - fraction) + sorted_values[upper] * fraction
    }
}

fn sorted_phases(report: &PerfReport) -> Vec<(&str, PhaseSummary)> {
    let mut phases: Vec<(&str, PhaseSummary)> = report
        .phase_totals
        .iter()
        .map(|(phase, summary)| (phase.as_str(), *summary))
        .collect();
    phases.sort_by(|left, right| right.1.total_ms.total_cmp(&left.1.total_ms));
    phases
}

fn percent_delta(baseline: f32, candidate: f32) -> f32 {
    if baseline.abs() <= f32::EPSILON {
        if candidate.abs() <= f32::EPSILON {
            0.0
        } else {
            100.0
        }
    } else {
        ((candidate - baseline) / baseline) * 100.0
    }
}

fn single_report_json(path: &str, report: &PerfReport) -> String {
    let mut root = Map::new();
    root.insert("path".to_string(), Value::String(path.to_string()));
    if let Some(session_id) = &report.session_id {
        root.insert("session_id".to_string(), Value::String(session_id.clone()));
    }
    insert_number(&mut root, "frames", report.frames as f64);
    insert_number(
        &mut root,
        "over_budget_frames",
        report.over_budget_frames as f64,
    );
    insert_number(&mut root, "worst_frame_ms", report.worst_frame_ms as f64);
    insert_number(
        &mut root,
        "average_frame_ms",
        report.average_frame_ms as f64,
    );
    if let Some(stats) = frame_stats(report) {
        root.insert("frame_stats".to_string(), metric_stats_json(stats));
    }
    if let Some(detail) = frame_detail_summary(report) {
        root.insert(
            "frame_detail".to_string(),
            frame_detail_summary_json(detail),
        );
    }
    root.insert(
        "bottlenecks".to_string(),
        Value::Array(
            bottleneck_findings(report)
                .into_iter()
                .map(bottleneck_finding_json)
                .collect(),
        ),
    );
    root.insert(
        "phases".to_string(),
        Value::Array(
            phase_stats(report)
                .into_iter()
                .map(phase_stats_json)
                .collect(),
        ),
    );
    Value::Object(root).to_string()
}

fn comparison_report_json(
    baseline_name: &str,
    baseline: &PerfReport,
    candidate_name: &str,
    candidate: &PerfReport,
) -> String {
    let mut root = Map::new();
    root.insert(
        "baseline".to_string(),
        Value::String(baseline_name.to_string()),
    );
    root.insert(
        "candidate".to_string(),
        Value::String(candidate_name.to_string()),
    );
    insert_number(
        &mut root,
        "average_frame_ms_delta",
        (candidate.average_frame_ms - baseline.average_frame_ms) as f64,
    );
    insert_number(
        &mut root,
        "worst_frame_ms_delta",
        (candidate.worst_frame_ms - baseline.worst_frame_ms) as f64,
    );
    if let (Some(left), Some(right)) = (frame_stats(baseline), frame_stats(candidate)) {
        insert_number(
            &mut root,
            "p95_frame_ms_delta",
            (right.p95 - left.p95) as f64,
        );
        insert_number(
            &mut root,
            "p99_frame_ms_delta",
            (right.p99 - left.p99) as f64,
        );
    }
    root.insert(
        "phase_deltas".to_string(),
        Value::Array(
            phase_deltas(baseline, candidate)
                .into_iter()
                .map(phase_delta_json)
                .collect(),
        ),
    );
    root.insert(
        "candidate_bottlenecks".to_string(),
        Value::Array(
            bottleneck_findings(candidate)
                .into_iter()
                .map(bottleneck_finding_json)
                .collect(),
        ),
    );
    Value::Object(root).to_string()
}

fn metric_stats_json(stats: MetricStats) -> Value {
    let mut object = Map::new();
    insert_number(&mut object, "min", stats.min as f64);
    insert_number(&mut object, "p50", stats.p50 as f64);
    insert_number(&mut object, "p90", stats.p90 as f64);
    insert_number(&mut object, "p95", stats.p95 as f64);
    insert_number(&mut object, "p99", stats.p99 as f64);
    insert_number(&mut object, "max", stats.max as f64);
    insert_number(&mut object, "average", stats.average as f64);
    insert_number(&mut object, "stddev", stats.stddev as f64);
    Value::Object(object)
}

fn frame_detail_summary_json(detail: FrameDetailSummary) -> Value {
    let mut object = Map::new();
    insert_number(&mut object, "start_frame", detail.start_frame as f64);
    insert_number(&mut object, "end_frame", detail.end_frame as f64);
    insert_number(
        &mut object,
        "average_profiled_phase_ms",
        detail.average_profiled_phase_ms as f64,
    );
    if let Some(value) = detail.latest_moving_average_ms {
        insert_number(&mut object, "latest_moving_average_ms", value as f64);
    }
    if let Some(value) = detail.max_budget_delta_ms {
        insert_number(&mut object, "max_budget_delta_ms", value as f64);
    }
    Value::Object(object)
}

fn phase_stats_json(stats: PhaseStats) -> Value {
    let mut object = Map::new();
    object.insert("phase".to_string(), Value::String(stats.phase));
    insert_number(&mut object, "average_ms", stats.average_ms as f64);
    insert_number(&mut object, "p95_ms", stats.p95_ms as f64);
    insert_number(&mut object, "p99_ms", stats.p99_ms as f64);
    insert_number(&mut object, "max_ms", stats.max_ms as f64);
    insert_number(&mut object, "samples", stats.samples as f64);
    Value::Object(object)
}

fn bottleneck_finding_json(finding: BottleneckFinding) -> Value {
    let mut object = Map::new();
    object.insert(
        "level".to_string(),
        Value::String(finding.level.to_string()),
    );
    object.insert("title".to_string(), Value::String(finding.title));
    object.insert("detail".to_string(), Value::String(finding.detail));
    Value::Object(object)
}

fn phase_delta_json(delta: PhaseDelta) -> Value {
    let mut object = Map::new();
    object.insert("phase".to_string(), Value::String(delta.phase));
    insert_number(
        &mut object,
        "baseline_average_ms",
        delta.baseline_average as f64,
    );
    insert_number(
        &mut object,
        "candidate_average_ms",
        delta.candidate_average as f64,
    );
    insert_number(&mut object, "average_delta_ms", delta.average_delta as f64);
    insert_number(&mut object, "baseline_p95_ms", delta.baseline_p95 as f64);
    insert_number(&mut object, "candidate_p95_ms", delta.candidate_p95 as f64);
    Value::Object(object)
}

fn insert_number(object: &mut Map<String, Value>, key: &str, value: f64) {
    if let Some(number) = serde_json::Number::from_f64(value) {
        object.insert(key.to_string(), Value::Number(number));
    }
}

fn latest_log_path(log_dir: &Path) -> Result<PathBuf, String> {
    let paths = sorted_log_paths(log_dir)?;
    paths
        .last()
        .cloned()
        .ok_or_else(|| format!("no performance logs found in {}", log_dir.display()))
}

fn latest_two_log_paths(log_dir: &Path) -> Result<(PathBuf, PathBuf), String> {
    let paths = sorted_log_paths(log_dir)?;
    if paths.len() < 2 {
        return Err(format!(
            "need at least two performance logs in {}",
            log_dir.display()
        ));
    }
    Ok((
        paths[paths.len() - 2].clone(),
        paths[paths.len() - 1].clone(),
    ))
}

fn sorted_log_paths(log_dir: &Path) -> Result<Vec<PathBuf>, String> {
    let entries = fs::read_dir(log_dir)
        .map_err(|error| format!("failed to read {}: {error}", log_dir.display()))?;
    let mut paths = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|error| format!("failed to read directory entry: {error}"))?;
        let path = entry.path();
        let Some(file_name) = path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        if file_name == DEFAULT_PERF_LOG_NAME || file_name.starts_with("performance.log.") {
            paths.push(path);
        }
    }
    paths.sort_by_key(|path| {
        fs::metadata(path)
            .and_then(|metadata| metadata.modified())
            .ok()
    });
    Ok(paths)
}

fn render_html_report(
    baseline_name: &str,
    baseline: &PerfReport,
    candidate: Option<(String, &PerfReport)>,
) -> Result<String, String> {
    let baseline_stats = frame_stats(baseline);
    let candidate_stats = candidate
        .as_ref()
        .and_then(|(_, report)| frame_stats(report));
    let baseline_series = downsample_frame_series(baseline);
    let candidate_series = candidate
        .as_ref()
        .map(|(_, report)| downsample_frame_series(report))
        .unwrap_or_default();
    let baseline_phase_labels: Vec<String> = phase_stats(baseline)
        .into_iter()
        .take(8)
        .map(|phase| phase.phase)
        .collect();
    let candidate_phase_labels: Vec<String> = candidate
        .as_ref()
        .map(|(_, report)| {
            phase_stats(report)
                .into_iter()
                .take(8)
                .map(|phase| phase.phase)
                .collect()
        })
        .unwrap_or_default();
    let all_phase_labels = merge_labels(&baseline_phase_labels, &candidate_phase_labels);
    let baseline_phase_values = phase_average_values(baseline, &all_phase_labels);
    let candidate_phase_values = candidate
        .as_ref()
        .map(|(_, report)| phase_average_values(report, &all_phase_labels))
        .unwrap_or_else(|| vec![0.0; all_phase_labels.len()]);

    let baseline_name = html_escape(baseline_name);
    let candidate_name = candidate
        .as_ref()
        .map(|(name, _)| html_escape(name))
        .unwrap_or_else(|| "none".to_string());

    let average_delta = candidate
        .as_ref()
        .map(|(_, report)| report.average_frame_ms - baseline.average_frame_ms)
        .unwrap_or(0.0);
    let p95_delta = match (baseline_stats, candidate_stats) {
        (Some(left), Some(right)) => right.p95 - left.p95,
        _ => 0.0,
    };
    let finding_panel = render_bottleneck_panel(
        "Bottleneck Diagnosis",
        &bottleneck_findings(
            candidate
                .as_ref()
                .map(|(_, report)| *report)
                .unwrap_or(baseline),
        ),
    );

    Ok(format!(
        r#"<!doctype html>
<html lang="zh-CN">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width, initial-scale=1">
  <title>Dao Performance Report</title>
  <style>
    :root {{
      color-scheme: light;
      --bg: #f6f7f9;
      --panel: #ffffff;
      --ink: #1d252c;
      --muted: #65717d;
      --line: #d8dee6;
      --accent: #18736b;
      --accent-2: #a94442;
      --accent-3: #2f6fbb;
    }}
    * {{ box-sizing: border-box; }}
    body {{
      margin: 0;
      font-family: "Segoe UI", system-ui, sans-serif;
      background: var(--bg);
      color: var(--ink);
    }}
    header {{
      padding: 28px 32px 18px;
      border-bottom: 1px solid var(--line);
      background: var(--panel);
    }}
    h1 {{
      margin: 0 0 8px;
      font-size: 28px;
      font-weight: 650;
      letter-spacing: 0;
    }}
    .meta {{
      color: var(--muted);
      font-size: 14px;
      line-height: 1.55;
      overflow-wrap: anywhere;
    }}
    main {{
      max-width: 1180px;
      margin: 0 auto;
      padding: 24px;
    }}
    .metrics {{
      display: grid;
      grid-template-columns: repeat(auto-fit, minmax(180px, 1fr));
      gap: 12px;
      margin-bottom: 18px;
    }}
    .metric {{
      background: var(--panel);
      border: 1px solid var(--line);
      border-radius: 8px;
      padding: 14px 16px;
      min-height: 92px;
    }}
    .label {{
      color: var(--muted);
      font-size: 13px;
      margin-bottom: 8px;
    }}
    .value {{
      font-size: 25px;
      font-weight: 650;
      line-height: 1.2;
    }}
    .delta-good {{ color: var(--accent); }}
    .delta-bad {{ color: var(--accent-2); }}
    .findings {{
      display: grid;
      gap: 10px;
      margin: 0;
      padding: 0;
      list-style: none;
    }}
    .finding {{
      border: 1px solid var(--line);
      border-left-width: 4px;
      border-radius: 6px;
      padding: 10px 12px;
      background: #fbfcfd;
    }}
    .finding.critical {{ border-left-color: var(--accent-2); }}
    .finding.warn {{ border-left-color: #b57918; }}
    .finding.info {{ border-left-color: var(--accent-3); }}
    .finding-title {{
      font-weight: 650;
      margin-bottom: 4px;
    }}
    .finding-detail {{
      color: var(--muted);
      font-size: 14px;
      line-height: 1.45;
    }}
    section {{
      background: var(--panel);
      border: 1px solid var(--line);
      border-radius: 8px;
      padding: 18px;
      margin-top: 16px;
    }}
    h2 {{
      margin: 0 0 12px;
      font-size: 18px;
      font-weight: 650;
      letter-spacing: 0;
    }}
    canvas {{
      width: 100%;
      height: 300px;
      display: block;
      border: 1px solid var(--line);
      border-radius: 6px;
      background: #fff;
    }}
    table {{
      width: 100%;
      border-collapse: collapse;
      font-size: 14px;
    }}
    th, td {{
      text-align: right;
      padding: 9px 8px;
      border-bottom: 1px solid var(--line);
      white-space: nowrap;
    }}
    th:first-child, td:first-child {{ text-align: left; }}
    @media (max-width: 720px) {{
      header {{ padding: 22px 18px 14px; }}
      main {{ padding: 16px; }}
      canvas {{ height: 240px; }}
      table {{ font-size: 13px; }}
    }}
  </style>
</head>
<body>
  <header>
    <h1>Dao Performance Report</h1>
    <div class="meta">Baseline: {baseline_name}<br>Candidate: {candidate_name}</div>
  </header>
  <main>
    <div class="metrics">
      <div class="metric"><div class="label">Baseline Avg</div><div class="value">{baseline_avg:.2} ms</div></div>
      <div class="metric"><div class="label">Candidate Avg Delta</div><div class="value {avg_delta_class}">{average_delta:+.2} ms</div></div>
      <div class="metric"><div class="label">Baseline P95</div><div class="value">{baseline_p95:.2} ms</div></div>
      <div class="metric"><div class="label">Candidate P95 Delta</div><div class="value {p95_delta_class}">{p95_delta:+.2} ms</div></div>
      <div class="metric"><div class="label">Over Budget</div><div class="value">{baseline_over_budget} / {candidate_over_budget}</div></div>
    </div>

    {finding_panel}

    <section>
      <h2>Frame Time Trend</h2>
      <canvas id="frameChart" width="1100" height="300"></canvas>
    </section>

    <section>
      <h2>Phase Average Comparison</h2>
      <canvas id="phaseChart" width="1100" height="300"></canvas>
    </section>

    <section>
      <h2>Phase Detail</h2>
      {phase_table}
    </section>
  </main>
  <script>
    const baselineSeries = {baseline_series};
    const candidateSeries = {candidate_series};
    const phaseLabels = {phase_labels};
    const baselinePhaseValues = {baseline_phase_values};
    const candidatePhaseValues = {candidate_phase_values};

    function drawLineChart(canvasId, seriesList, labels) {{
      const canvas = document.getElementById(canvasId);
      const ctx = canvas.getContext('2d');
      const width = canvas.width;
      const height = canvas.height;
      const pad = 38;
      ctx.clearRect(0, 0, width, height);
      ctx.strokeStyle = '#d8dee6';
      ctx.lineWidth = 1;
      ctx.beginPath();
      ctx.moveTo(pad, pad);
      ctx.lineTo(pad, height - pad);
      ctx.lineTo(width - pad, height - pad);
      ctx.stroke();
      const values = seriesList.flat();
      const max = Math.max(1, ...values) * 1.1;
      for (let g = 0; g <= 4; g++) {{
        const y = pad + (height - pad * 2) * g / 4;
        ctx.strokeStyle = '#edf0f3';
        ctx.beginPath();
        ctx.moveTo(pad, y);
        ctx.lineTo(width - pad, y);
        ctx.stroke();
      }}
      const colors = ['#18736b', '#a94442'];
      seriesList.forEach((series, index) => {{
        if (!series.length) return;
        ctx.strokeStyle = colors[index];
        ctx.lineWidth = 2;
        ctx.beginPath();
        series.forEach((value, i) => {{
          const x = pad + (width - pad * 2) * i / Math.max(1, series.length - 1);
          const y = height - pad - (height - pad * 2) * value / max;
          if (i === 0) ctx.moveTo(x, y); else ctx.lineTo(x, y);
        }});
        ctx.stroke();
      }});
      ctx.fillStyle = '#65717d';
      ctx.font = '13px Segoe UI, sans-serif';
      labels.forEach((label, index) => ctx.fillText(label, pad + index * 120, 22));
      ctx.fillText(max.toFixed(1) + ' ms', 8, pad + 4);
      ctx.fillText('0 ms', 8, height - pad + 4);
    }}

    function drawBarChart(canvasId, labels, leftValues, rightValues) {{
      const canvas = document.getElementById(canvasId);
      const ctx = canvas.getContext('2d');
      const width = canvas.width;
      const height = canvas.height;
      const pad = 48;
      const max = Math.max(1, ...leftValues, ...rightValues) * 1.18;
      ctx.clearRect(0, 0, width, height);
      ctx.strokeStyle = '#d8dee6';
      ctx.beginPath();
      ctx.moveTo(pad, pad);
      ctx.lineTo(pad, height - pad);
      ctx.lineTo(width - pad, height - pad);
      ctx.stroke();
      const groupWidth = (width - pad * 2) / Math.max(1, labels.length);
      labels.forEach((label, i) => {{
        const x = pad + i * groupWidth;
        const leftH = (height - pad * 2) * leftValues[i] / max;
        const rightH = (height - pad * 2) * rightValues[i] / max;
        ctx.fillStyle = '#18736b';
        ctx.fillRect(x + groupWidth * 0.22, height - pad - leftH, groupWidth * 0.2, leftH);
        ctx.fillStyle = '#a94442';
        ctx.fillRect(x + groupWidth * 0.48, height - pad - rightH, groupWidth * 0.2, rightH);
        ctx.save();
        ctx.translate(x + groupWidth * 0.5, height - 12);
        ctx.rotate(-0.42);
        ctx.fillStyle = '#65717d';
        ctx.font = '12px Segoe UI, sans-serif';
        ctx.textAlign = 'right';
        ctx.fillText(label, 0, 0);
        ctx.restore();
      }});
      ctx.fillStyle = '#65717d';
      ctx.font = '13px Segoe UI, sans-serif';
      ctx.fillText(max.toFixed(1) + ' ms', 8, pad + 4);
    }}

    drawLineChart('frameChart', [baselineSeries, candidateSeries], ['baseline', 'candidate']);
    drawBarChart('phaseChart', phaseLabels, baselinePhaseValues, candidatePhaseValues);
  </script>
</body>
</html>"#,
        baseline_avg = baseline.average_frame_ms,
        average_delta = average_delta,
        baseline_p95 = baseline_stats.map(|stats| stats.p95).unwrap_or(0.0),
        p95_delta = p95_delta,
        baseline_over_budget = baseline.over_budget_frames,
        candidate_over_budget = candidate
            .as_ref()
            .map(|(_, report)| report.over_budget_frames)
            .unwrap_or(0),
        avg_delta_class = delta_class(average_delta),
        p95_delta_class = delta_class(p95_delta),
        finding_panel = finding_panel,
        baseline_series = number_array_json(&baseline_series)?,
        candidate_series = number_array_json(&candidate_series)?,
        phase_labels = string_array_json(&all_phase_labels)?,
        baseline_phase_values = number_array_json(&baseline_phase_values)?,
        candidate_phase_values = number_array_json(&candidate_phase_values)?,
        phase_table = render_phase_table(baseline, candidate.as_ref().map(|(_, report)| *report)),
    ))
}

fn downsample_frame_series(report: &PerfReport) -> Vec<f32> {
    if report.frame_samples.is_empty() {
        return Vec::new();
    }
    if report.frame_samples.len() <= HTML_CHART_POINTS {
        return report
            .frame_samples
            .iter()
            .map(|sample| sample.frame_ms)
            .collect();
    }
    let bucket = (report.frame_samples.len() as f32 / HTML_CHART_POINTS as f32).ceil() as usize;
    report
        .frame_samples
        .chunks(bucket)
        .map(|chunk| chunk.iter().map(|sample| sample.frame_ms).sum::<f32>() / chunk.len() as f32)
        .collect()
}

fn merge_labels(left: &[String], right: &[String]) -> Vec<String> {
    let mut labels = left.to_vec();
    labels.extend(right.iter().cloned());
    labels.sort();
    labels.dedup();
    labels.truncate(10);
    labels
}

fn phase_average_values(report: &PerfReport, labels: &[String]) -> Vec<f32> {
    let stats = phase_stats_by_name(report);
    labels
        .iter()
        .map(|label| {
            stats
                .get(label)
                .map(|stats| stats.average_ms)
                .unwrap_or(0.0)
        })
        .collect()
}

fn render_phase_table(baseline: &PerfReport, candidate: Option<&PerfReport>) -> String {
    let baseline_stats = phase_stats_by_name(baseline);
    let candidate_stats = candidate.map(phase_stats_by_name).unwrap_or_default();
    let mut labels: Vec<String> = baseline_stats
        .keys()
        .chain(candidate_stats.keys())
        .cloned()
        .collect();
    labels.sort();
    labels.dedup();

    let mut rows = String::new();
    for label in labels {
        let baseline_average = baseline_stats
            .get(&label)
            .map(|stats| stats.average_ms)
            .unwrap_or(0.0);
        let baseline_p95 = baseline_stats
            .get(&label)
            .map(|stats| stats.p95_ms)
            .unwrap_or(0.0);
        let candidate_average = candidate_stats
            .get(&label)
            .map(|stats| stats.average_ms)
            .unwrap_or(0.0);
        let candidate_p95 = candidate_stats
            .get(&label)
            .map(|stats| stats.p95_ms)
            .unwrap_or(0.0);
        let delta = candidate_average - baseline_average;
        rows.push_str(&format!(
            "<tr><td>{}</td><td>{:.2}</td><td>{:.2}</td><td>{:.2}</td><td>{:.2}</td><td class=\"{}\">{:+.2}</td></tr>",
            html_escape(&label),
            baseline_average,
            baseline_p95,
            candidate_average,
            candidate_p95,
            delta_class(delta),
            delta
        ));
    }

    format!(
        "<table><thead><tr><th>Phase</th><th>Baseline Avg</th><th>Baseline P95</th><th>Candidate Avg</th><th>Candidate P95</th><th>Avg Delta</th></tr></thead><tbody>{rows}</tbody></table>"
    )
}

fn render_bottleneck_panel(title: &str, findings: &[BottleneckFinding]) -> String {
    let mut rows = String::new();
    for finding in findings {
        rows.push_str(&format!(
            "<li class=\"finding {}\"><div class=\"finding-title\">[{}] {}</div><div class=\"finding-detail\">{}</div></li>",
            html_escape(finding.level),
            html_escape(finding.level),
            html_escape(&finding.title),
            html_escape(&finding.detail)
        ));
    }
    format!(
        "<section><h2>{}</h2><ul class=\"findings\">{rows}</ul></section>",
        html_escape(title)
    )
}

fn delta_class(delta: f32) -> &'static str {
    if delta > 0.0 {
        "delta-bad"
    } else {
        "delta-good"
    }
}

fn html_escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn number_array_json(values: &[f32]) -> Result<String, String> {
    serde_json::to_string(values).map_err(|error| format!("failed to encode chart data: {error}"))
}

fn string_array_json(values: &[String]) -> Result<String, String> {
    serde_json::to_string(values).map_err(|error| format!("failed to encode chart labels: {error}"))
}

fn print_usage() {
    println!("{}", usage_error());
}

fn usage_error() -> String {
    "usage:
  cargo run --bin perf_report -- <log.json>
  cargo run --bin perf_report -- <baseline_log.json> <candidate_log.json>
  cargo run --bin perf_report -- latest [log_dir]
  cargo run --bin perf_report -- compare-latest [log_dir]
  cargo run --bin perf_report -- html <output.html> <log.json> [candidate_log.json]
  cargo run --bin perf_report -- html-latest <output.html> [log_dir]
options:
  --json    print machine-readable JSON for text commands"
        .to_string()
}

#[cfg(test)]
mod tests {
    use std::{
        fs,
        path::PathBuf,
        time::{SystemTime, UNIX_EPOCH},
    };

    use super::{Command, bottleneck_findings, load_report, metric_stats, render_html_report};

    #[test]
    fn load_report_uses_latest_session_and_accepts_string_or_numeric_session_ids() {
        let path = unique_temp_path("perf-report-session");
        fs::write(
            &path,
            concat!(
                "{\"target\":\"dao_game::performance::session_start\",\"fields\":{\"session_id\":\"42\"}}\n",
                "{\"target\":\"dao_game::performance::budget\",\"fields\":{\"session_id\":\"42\",\"frame_ms\":20.0,\"top_phase_1_name\":\"ui\",\"top_phase_1_ms\":1.0}}\n",
                "{\"target\":\"dao_game::performance::session\",\"fields\":{\"session_id\":\"42\",\"frames\":10,\"over_budget_frames\":1,\"worst_frame_ms\":20.0,\"average_frame_ms\":15.0,\"average_over_budget_frame_ms\":20.0,\"hot_phase_1_name\":\"ui\",\"hot_phase_1_avg_ms\":1.5,\"hot_phase_1_max_ms\":2.0}}\n",
                "{\"target\":\"dao_game::performance::session_start\",\"fields\":{\"session_id\":43}}\n",
                "{\"target\":\"dao_game::performance::budget\",\"fields\":{\"session_id\":43,\"frame_ms\":30.0,\"top_phase_1_name\":\"environment\",\"top_phase_1_ms\":3.0}}\n",
                "{\"target\":\"dao_game::performance::session\",\"fields\":{\"session_id\":43,\"frames\":5,\"over_budget_frames\":2,\"worst_frame_ms\":40.0,\"average_frame_ms\":18.0,\"average_over_budget_frame_ms\":35.0,\"hot_phase_1_name\":\"environment\",\"hot_phase_1_avg_ms\":2.5,\"hot_phase_1_max_ms\":5.0}}\n"
            ),
        )
        .unwrap();

        let report = load_report(&path).unwrap();

        assert_eq!(report.session_id.as_deref(), Some("43"));
        assert_eq!(report.frames, 5);
        assert_eq!(report.over_budget_frames, 2);
        assert_eq!(report.worst_frame_ms, 40.0);
        assert_eq!(report.average_frame_ms, 18.0);
        assert_eq!(report.average_over_budget_frame_ms, 35.0);
        let environment = report.phase_totals.get("environment").unwrap();
        assert_eq!(environment.max_ms, 5.0);
        assert!((environment.total_ms - 12.5).abs() < f32::EPSILON);

        let _ = fs::remove_file(path);
    }

    #[test]
    fn load_report_prefers_frame_detail_samples_for_distribution_stats() {
        let path = unique_temp_path("perf-report-detail");
        fs::write(
            &path,
            concat!(
                "{\"target\":\"dao_game::performance::session_start\",\"fields\":{\"session_id\":\"77\",\"budget_ms\":16.6}}\n",
                "{\"target\":\"dao_game::performance::frame_detail\",\"fields\":{\"session_id\":\"77\",\"frame\":1,\"frame_ms\":10.0,\"average_ms\":10.0,\"budget_ms\":16.6,\"profiled_phase_ms\":3.0,\"ui_ms\":1.0,\"world_streaming_ms\":2.0}}\n",
                "{\"target\":\"dao_game::performance::frame_detail\",\"fields\":{\"session_id\":\"77\",\"frame\":2,\"frame_ms\":20.0,\"average_ms\":11.0,\"budget_ms\":16.6,\"profiled_phase_ms\":6.0,\"ui_ms\":2.0,\"world_streaming_ms\":4.0}}\n",
                "{\"target\":\"dao_game::performance::session\",\"fields\":{\"session_id\":\"77\",\"frames\":2,\"over_budget_frames\":1,\"worst_frame_ms\":20.0,\"average_frame_ms\":15.0,\"average_over_budget_frame_ms\":20.0,\"hot_phase_1_name\":\"world_streaming\",\"hot_phase_1_avg_ms\":3.0,\"hot_phase_1_max_ms\":4.0}}\n"
            ),
        )
        .unwrap();

        let report = load_report(&path).unwrap();

        assert_eq!(report.frames, 2);
        assert_eq!(report.over_budget_frames, 1);
        assert_eq!(report.average_frame_ms, 15.0);
        assert_eq!(report.average_over_budget_frame_ms, 20.0);
        assert_eq!(report.frame_samples.len(), 2);
        let streaming = report.phase_totals.get("world_streaming").unwrap();
        assert_eq!(streaming.total_ms, 6.0);
        assert_eq!(streaming.max_ms, 4.0);

        let _ = fs::remove_file(path);
    }

    #[test]
    fn load_report_skips_invalid_lines() {
        let path = unique_temp_path("perf-report-invalid");
        fs::write(
            &path,
            concat!(
                "not-json\n",
                "{\"target\":\"dao_game::performance::session_start\",\"fields\":{\"session_id\":\"77\"}}\n",
                "{\"target\":\"dao_game::performance::budget\",\"fields\":{\"session_id\":\"77\",\"frame_ms\":25.0,\"top_phase_1_name\":\"world_streaming\",\"top_phase_1_ms\":2.0}}\n"
            ),
        )
        .unwrap();

        let report = load_report(&path).unwrap();

        assert_eq!(report.session_id.as_deref(), Some("77"));
        assert_eq!(report.over_budget_frames, 1);
        assert_eq!(report.ignored_lines, 1);
        assert_eq!(report.average_over_budget_frame_ms, 25.0);

        let _ = fs::remove_file(path);
    }

    #[test]
    fn load_report_accepts_utf8_bom_on_first_line() {
        let path = unique_temp_path("perf-report-bom");
        fs::write(
            &path,
            concat!(
                "\u{feff}{\"target\":\"dao_game::performance::session_start\",\"fields\":{\"session_id\":\"88\"}}\n",
                "{\"target\":\"dao_game::performance::frame_detail\",\"fields\":{\"session_id\":\"88\",\"frame\":1,\"frame_ms\":12.0}}\n"
            ),
        )
        .unwrap();

        let report = load_report(&path).unwrap();

        assert_eq!(report.ignored_lines, 0);
        assert_eq!(report.session_id.as_deref(), Some("88"));
        assert_eq!(report.frames, 1);

        let _ = fs::remove_file(path);
    }

    #[test]
    fn metric_stats_calculates_percentiles_and_stddev() {
        let stats = metric_stats(&[10.0, 20.0, 30.0, 40.0, 50.0]);

        assert_eq!(stats.min, 10.0);
        assert_eq!(stats.p50, 30.0);
        assert_eq!(stats.p90, 46.0);
        assert_eq!(stats.max, 50.0);
        assert!(stats.stddev > 0.0);
    }

    #[test]
    fn command_parser_keeps_latest_subcommands_reachable() {
        let latest = Command::parse(&["latest".to_string()]).unwrap();
        let compare_latest = Command::parse(&["compare-latest".to_string()]).unwrap();

        assert!(matches!(latest, Command::Latest { .. }));
        assert!(matches!(compare_latest, Command::CompareLatest { .. }));
    }

    #[test]
    fn html_report_contains_chart_data() {
        let path = unique_temp_path("perf-report-html");
        fs::write(
            &path,
            concat!(
                "{\"target\":\"dao_game::performance::session_start\",\"fields\":{\"session_id\":\"77\",\"budget_ms\":16.6}}\n",
                "{\"target\":\"dao_game::performance::frame_detail\",\"fields\":{\"session_id\":\"77\",\"frame\":1,\"frame_ms\":10.0,\"average_ms\":10.0,\"budget_ms\":16.6,\"profiled_phase_ms\":3.0,\"ui_ms\":1.0,\"world_streaming_ms\":2.0}}\n",
                "{\"target\":\"dao_game::performance::frame_detail\",\"fields\":{\"session_id\":\"77\",\"frame\":2,\"frame_ms\":18.0,\"average_ms\":10.8,\"budget_ms\":16.6,\"profiled_phase_ms\":4.0,\"ui_ms\":1.5,\"world_streaming_ms\":2.5}}\n"
            ),
        )
        .unwrap();
        let report = load_report(&path).unwrap();

        let html = render_html_report("baseline", &report, None).unwrap();

        assert!(html.contains("Frame Time Trend"));
        assert!(html.contains("Bottleneck Diagnosis"));
        assert!(html.contains("baselineSeries"));
        assert!(html.contains("world_streaming"));

        let _ = fs::remove_file(path);
    }

    #[test]
    fn bottleneck_findings_call_out_budget_and_hot_phase() {
        let path = unique_temp_path("perf-report-bottleneck");
        fs::write(
            &path,
            concat!(
                "{\"target\":\"dao_game::performance::session_start\",\"fields\":{\"session_id\":\"91\",\"budget_ms\":16.6}}\n",
                "{\"target\":\"dao_game::performance::frame_detail\",\"fields\":{\"session_id\":\"91\",\"frame\":1,\"frame_ms\":17.0,\"average_ms\":17.0,\"budget_ms\":16.6,\"budget_delta_ms\":0.4,\"profiled_phase_ms\":10.0,\"world_streaming_ms\":9.0,\"ui_ms\":1.0}}\n",
                "{\"target\":\"dao_game::performance::frame_detail\",\"fields\":{\"session_id\":\"91\",\"frame\":2,\"frame_ms\":22.0,\"average_ms\":17.5,\"budget_ms\":16.6,\"budget_delta_ms\":5.4,\"profiled_phase_ms\":13.0,\"world_streaming_ms\":12.0,\"ui_ms\":1.0}}\n",
                "{\"target\":\"dao_game::performance::frame_detail\",\"fields\":{\"session_id\":\"91\",\"frame\":3,\"frame_ms\":30.0,\"average_ms\":18.7,\"budget_ms\":16.6,\"budget_delta_ms\":13.4,\"profiled_phase_ms\":21.0,\"world_streaming_ms\":20.0,\"ui_ms\":1.0}}\n"
            ),
        )
        .unwrap();
        let report = load_report(&path).unwrap();

        let findings = bottleneck_findings(&report);

        assert!(
            findings
                .iter()
                .any(|finding| finding.title.contains("frame budget exceeded"))
        );
        assert!(
            findings
                .iter()
                .any(|finding| finding.title.contains("world_streaming"))
        );

        let _ = fs::remove_file(path);
    }

    fn unique_temp_path(prefix: &str) -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("{prefix}-{unique}.log"))
    }
}
