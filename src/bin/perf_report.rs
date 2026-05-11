use std::{
    collections::HashMap,
    env,
    fs::File,
    io::{BufRead, BufReader},
    path::Path,
};

use serde::Deserialize;
use serde_json::{Map, Value};

fn main() -> Result<(), String> {
    let args: Vec<String> = env::args().skip(1).collect();
    match args.as_slice() {
        [path] => {
            let report = load_report(Path::new(path))?;
            print_single_report(path, &report);
            Ok(())
        }
        [baseline, candidate] => {
            let baseline_report = load_report(Path::new(baseline))?;
            let candidate_report = load_report(Path::new(candidate))?;
            print_comparison_report(baseline, &baseline_report, candidate, &candidate_report);
            Ok(())
        }
        _ => {
            Err("usage: cargo run --bin perf_report -- <log.json> [candidate_log.json]".to_string())
        }
    }
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
    phase_totals: HashMap<String, PhaseSummary>,
    ignored_lines: u64,
}

#[derive(Debug, Default, Clone, Copy)]
struct PhaseSummary {
    total_ms: f32,
    max_ms: f32,
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
        if line.trim().is_empty() {
            continue;
        }
        let parsed: PerfLogLine = match serde_json::from_str(&line) {
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
            continue;
        }
        if active_session_id.is_some() && line_session_id != active_session_id {
            continue;
        }
        match parsed.target.as_str() {
            "dao_game::performance::budget" => {
                if let Some(frame_ms) = parse_f32_field(fields, "frame_ms") {
                    report.frames += 1;
                    report.over_budget_frames += 1;
                    report.worst_frame_ms = report.worst_frame_ms.max(frame_ms);
                    report.sampled_over_budget_total_ms += frame_ms;
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
                        }
                    }
                }
            }
            "dao_game::performance::session" => {
                report.session_id = line_session_id.or(report.session_id);
                report.frames = parse_u64_field(fields, "frames").unwrap_or(report.frames);
                report.over_budget_frames = parse_u64_field(fields, "over_budget_frames")
                    .unwrap_or(report.over_budget_frames);
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
                            },
                        );
                    }
                }
            }
            _ => {}
        }
    }

    if report.over_budget_frames > 0 && report.average_over_budget_frame_ms == 0.0 {
        report.average_over_budget_frame_ms =
            report.sampled_over_budget_total_ms / report.over_budget_frames as f32;
    }

    Ok(report)
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

fn print_single_report(path: &str, report: &PerfReport) {
    println!("Report: {path}");
    if let Some(session_id) = &report.session_id {
        println!("session_id: {session_id}");
    }
    println!("frames: {}", report.frames);
    println!("over_budget_frames: {}", report.over_budget_frames);
    println!("worst_frame_ms: {:.2}", report.worst_frame_ms);
    println!("average_frame_ms: {:.2}", report.average_frame_ms);
    println!(
        "average_over_budget_frame_ms: {:.2}",
        report.average_over_budget_frame_ms
    );
    if report.ignored_lines > 0 {
        println!("ignored_lines: {}", report.ignored_lines);
    }
    println!("top_phases:");
    for (phase, summary) in sorted_phases(report) {
        let avg_ms = if report.frames > 0 {
            summary.total_ms / report.frames as f32
        } else {
            0.0
        };
        println!(
            "  {phase}: avg {:.2} ms, max {:.2} ms",
            avg_ms, summary.max_ms
        );
    }
}

fn print_comparison_report(
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
        "worst_frame_ms: {:.2} -> {:.2} ({:+.2})",
        baseline.worst_frame_ms,
        candidate.worst_frame_ms,
        candidate.worst_frame_ms - baseline.worst_frame_ms
    );
    println!(
        "average_frame_ms: {:.2} -> {:.2} ({:+.2})",
        baseline.average_frame_ms,
        candidate.average_frame_ms,
        candidate.average_frame_ms - baseline.average_frame_ms
    );
    println!(
        "over_budget_frames: {} -> {} ({:+})",
        baseline.over_budget_frames,
        candidate.over_budget_frames,
        candidate.over_budget_frames as i64 - baseline.over_budget_frames as i64
    );
    println!(
        "average_over_budget_frame_ms: {:.2} -> {:.2} ({:+.2})",
        baseline.average_over_budget_frame_ms,
        candidate.average_over_budget_frame_ms,
        candidate.average_over_budget_frame_ms - baseline.average_over_budget_frame_ms
    );

    let mut phase_names: Vec<String> = baseline
        .phase_totals
        .keys()
        .chain(candidate.phase_totals.keys())
        .cloned()
        .collect();
    phase_names.sort();
    phase_names.dedup();

    println!("phase_deltas:");
    for phase in phase_names {
        let baseline_avg = baseline
            .phase_totals
            .get(&phase)
            .map(|summary| summary.total_ms / baseline.frames.max(1) as f32)
            .unwrap_or(0.0);
        let candidate_avg = candidate
            .phase_totals
            .get(&phase)
            .map(|summary| summary.total_ms / candidate.frames.max(1) as f32)
            .unwrap_or(0.0);
        println!(
            "  {phase}: {:.2} -> {:.2} ms ({:+.2})",
            baseline_avg,
            candidate_avg,
            candidate_avg - baseline_avg
        );
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

#[cfg(test)]
mod tests {
    use std::{
        fs,
        path::PathBuf,
        time::{SystemTime, UNIX_EPOCH},
    };

    use super::load_report;

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

    fn unique_temp_path(prefix: &str) -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("{prefix}-{unique}.log"))
    }
}
