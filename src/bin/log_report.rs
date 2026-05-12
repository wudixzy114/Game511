use std::{
    collections::{BTreeSet, HashMap},
    env, fs,
    fs::File,
    io::{BufRead, BufReader},
    path::{Path, PathBuf},
};

use serde::Deserialize;
use serde_json::Value;

const DEFAULT_LOG_DIR: &str = "logs";
const DEFAULT_OUTPUT: &str = "logs/log-report.html";
const LOG_FILES: [&str; 6] = [
    "application.log",
    "application.log.1",
    "error.log",
    "error.log.1",
    "performance.log",
    "performance.log.1",
];

fn main() -> Result<(), String> {
    let args: Vec<String> = env::args().skip(1).collect();
    let command = Command::parse(&args)?;
    match command {
        Command::Html { log_dir, output } => {
            let entries = load_logs(&log_dir)?;
            let html = render_html(&entries)?;
            if let Some(parent) = output
                .parent()
                .filter(|parent| !parent.as_os_str().is_empty())
            {
                fs::create_dir_all(parent)
                    .map_err(|error| format!("failed to create {}: {error}", parent.display()))?;
            }
            fs::write(&output, html)
                .map_err(|error| format!("failed to write {}: {error}", output.display()))?;
            println!("Log HTML report written: {}", output.display());
            Ok(())
        }
        Command::Help => {
            print_usage();
            Ok(())
        }
    }
}

#[derive(Debug, Clone)]
enum Command {
    Html { log_dir: PathBuf, output: PathBuf },
    Help,
}

impl Command {
    fn parse(args: &[String]) -> Result<Self, String> {
        if args.iter().any(|arg| arg == "-h" || arg == "--help") {
            return Ok(Self::Help);
        }

        let mut log_dir = PathBuf::from(DEFAULT_LOG_DIR);
        let mut output = PathBuf::from(DEFAULT_OUTPUT);
        let mut index = 0;
        while index < args.len() {
            match args[index].as_str() {
                "--log-dir" => {
                    index += 1;
                    let Some(value) = args.get(index) else {
                        return Err("missing value for --log-dir".to_string());
                    };
                    log_dir = PathBuf::from(value);
                }
                "--output" => {
                    index += 1;
                    let Some(value) = args.get(index) else {
                        return Err("missing value for --output".to_string());
                    };
                    output = PathBuf::from(value);
                }
                value => return Err(format!("unknown argument: {value}\n{}", usage())),
            }
            index += 1;
        }

        Ok(Self::Html { log_dir, output })
    }
}

#[derive(Debug, Clone)]
struct LogEntry {
    source: String,
    log_type: String,
    level: String,
    target: String,
    timestamp: String,
    message: String,
    fields: String,
    raw: String,
}

#[derive(Debug, Deserialize)]
struct JsonLogLine {
    #[serde(default)]
    timestamp: Option<String>,
    #[serde(default)]
    level: Option<String>,
    #[serde(default)]
    target: Option<String>,
    #[serde(default)]
    fields: Option<Value>,
    #[serde(default)]
    message: Option<String>,
}

fn load_logs(log_dir: &Path) -> Result<Vec<LogEntry>, String> {
    let mut entries = Vec::new();
    for file_name in LOG_FILES {
        let path = log_dir.join(file_name);
        if !path.exists() {
            continue;
        }
        entries.extend(load_log_file(&path, file_name)?);
    }
    Ok(entries)
}

fn load_log_file(path: &Path, file_name: &str) -> Result<Vec<LogEntry>, String> {
    let file =
        File::open(path).map_err(|error| format!("failed to open {}: {error}", path.display()))?;
    let reader = BufReader::new(file);
    let mut entries = Vec::new();
    let log_type = classify_log_type(file_name).to_string();
    for line in reader.lines() {
        let line = line.map_err(|error| format!("failed to read {}: {error}", path.display()))?;
        let line = line.trim_start_matches('\u{feff}');
        if line.trim().is_empty() {
            continue;
        }
        entries.push(parse_log_line(file_name, &log_type, line));
    }
    Ok(entries)
}

fn parse_log_line(source: &str, log_type: &str, line: &str) -> LogEntry {
    match serde_json::from_str::<JsonLogLine>(line) {
        Ok(parsed) => {
            let (message, fields) = extract_message_and_fields(parsed.message, parsed.fields);
            LogEntry {
                source: source.to_string(),
                log_type: log_type.to_string(),
                level: parsed.level.unwrap_or_else(|| "UNKNOWN".to_string()),
                target: parsed.target.unwrap_or_else(|| "unknown".to_string()),
                timestamp: parsed.timestamp.unwrap_or_default(),
                message,
                fields,
                raw: line.to_string(),
            }
        }
        Err(_) => LogEntry {
            source: source.to_string(),
            log_type: log_type.to_string(),
            level: infer_text_level(line).to_string(),
            target: "text".to_string(),
            timestamp: String::new(),
            message: line.to_string(),
            fields: String::new(),
            raw: line.to_string(),
        },
    }
}

fn extract_message_and_fields(message: Option<String>, fields: Option<Value>) -> (String, String) {
    let Some(fields) = fields else {
        return (message.unwrap_or_default(), String::new());
    };
    let field_message = fields
        .get("message")
        .and_then(Value::as_str)
        .map(ToOwned::to_owned);
    let message = message.or(field_message).unwrap_or_default();
    let fields = match fields {
        Value::Object(mut object) => {
            object.remove("message");
            Value::Object(object).to_string()
        }
        value => value.to_string(),
    };
    (message, fields)
}

fn classify_log_type(file_name: &str) -> &'static str {
    if file_name.starts_with("performance") {
        "performance"
    } else if file_name.starts_with("error") {
        "error"
    } else {
        "application"
    }
}

fn infer_text_level(line: &str) -> &'static str {
    for level in ["ERROR", "WARN", "INFO", "DEBUG", "TRACE"] {
        if line.contains(level) {
            return level;
        }
    }
    "UNKNOWN"
}

fn render_html(entries: &[LogEntry]) -> Result<String, String> {
    let types = unique_values(entries.iter().map(|entry| entry.log_type.as_str()));
    let levels = unique_values(entries.iter().map(|entry| entry.level.as_str()));
    let targets = unique_values(entries.iter().map(|entry| entry.target.as_str()));
    let counts = counts_by_type(entries);
    let data = serde_json::to_string(&entries_json(entries))
        .map_err(|error| format!("failed to encode log entries: {error}"))?;
    let type_options = render_options(&types);
    let level_options = render_options(&levels);
    let target_options = render_options(&targets);
    let summary = render_summary(entries.len(), &counts);

    Ok(format!(
        r#"<!doctype html>
<html lang="zh-CN">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width, initial-scale=1">
  <title>Dao Log Report</title>
  <style>
    :root {{
      --bg: #f5f6f8;
      --panel: #ffffff;
      --ink: #182029;
      --muted: #66727e;
      --line: #d9e0e7;
      --accent: #236d70;
      --warn: #9c6b1c;
      --error: #a83e3e;
      --debug: #4e6485;
    }}
    * {{ box-sizing: border-box; }}
    body {{
      margin: 0;
      background: var(--bg);
      color: var(--ink);
      font-family: "Segoe UI", system-ui, sans-serif;
    }}
    header {{
      background: var(--panel);
      border-bottom: 1px solid var(--line);
      padding: 24px 28px 16px;
    }}
    h1 {{
      margin: 0 0 8px;
      font-size: 28px;
      letter-spacing: 0;
    }}
    .meta {{ color: var(--muted); font-size: 14px; }}
    main {{ max-width: 1280px; margin: 0 auto; padding: 20px; }}
    .summary {{
      display: grid;
      grid-template-columns: repeat(auto-fit, minmax(150px, 1fr));
      gap: 10px;
      margin-bottom: 14px;
    }}
    .metric {{
      background: var(--panel);
      border: 1px solid var(--line);
      border-radius: 8px;
      padding: 12px;
    }}
    .metric .label {{ color: var(--muted); font-size: 12px; }}
    .metric .value {{ font-size: 22px; font-weight: 650; margin-top: 4px; }}
    .filters {{
      display: grid;
      grid-template-columns: repeat(4, minmax(140px, 1fr));
      gap: 10px;
      background: var(--panel);
      border: 1px solid var(--line);
      border-radius: 8px;
      padding: 12px;
      margin-bottom: 14px;
      position: sticky;
      top: 0;
      z-index: 2;
    }}
    select, input {{
      width: 100%;
      border: 1px solid var(--line);
      border-radius: 6px;
      padding: 8px 9px;
      font: inherit;
      background: #fff;
      color: var(--ink);
    }}
    table {{
      width: 100%;
      border-collapse: collapse;
      background: var(--panel);
      border: 1px solid var(--line);
      border-radius: 8px;
      overflow: hidden;
      table-layout: fixed;
    }}
    th, td {{
      border-bottom: 1px solid var(--line);
      padding: 8px;
      text-align: left;
      vertical-align: top;
      font-size: 13px;
      overflow-wrap: anywhere;
    }}
    th {{ color: var(--muted); background: #fbfcfd; font-weight: 650; }}
    .col-time {{ width: 170px; }}
    .col-level {{ width: 76px; }}
    .col-type {{ width: 100px; }}
    .col-source {{ width: 150px; }}
    .level-ERROR {{ color: var(--error); font-weight: 650; }}
    .level-WARN {{ color: var(--warn); font-weight: 650; }}
    .level-DEBUG, .level-TRACE {{ color: var(--debug); }}
    .message {{ font-weight: 550; margin-bottom: 4px; }}
    .fields {{ color: var(--muted); font-family: Consolas, monospace; font-size: 12px; }}
    .hidden {{ display: none; }}
    @media (max-width: 900px) {{
      .filters {{ grid-template-columns: 1fr 1fr; }}
      .col-source {{ display: none; }}
      th.col-source, td.col-source {{ display: none; }}
    }}
  </style>
</head>
<body>
  <header>
    <h1>Dao Log Report</h1>
    <div class="meta">Current and previous rotated logs. Use filters to isolate level, type, system target, or text.</div>
  </header>
  <main>
    {summary}
    <div class="filters">
      <select id="typeFilter"><option value="">All types</option>{type_options}</select>
      <select id="levelFilter"><option value="">All levels</option>{level_options}</select>
      <select id="targetFilter"><option value="">All systems</option>{target_options}</select>
      <input id="searchFilter" type="search" placeholder="Search message, fields, target">
    </div>
    <table>
      <thead>
        <tr>
          <th class="col-time">Time</th>
          <th class="col-level">Level</th>
          <th class="col-type">Type</th>
          <th>System</th>
          <th>Message / Fields</th>
          <th class="col-source">File</th>
        </tr>
      </thead>
      <tbody id="logRows"></tbody>
    </table>
  </main>
  <script>
    const entries = {data};
    const rows = document.getElementById('logRows');
    const typeFilter = document.getElementById('typeFilter');
    const levelFilter = document.getElementById('levelFilter');
    const targetFilter = document.getElementById('targetFilter');
    const searchFilter = document.getElementById('searchFilter');

    function escapeHtml(value) {{
      return String(value ?? '').replace(/[&<>"']/g, c => ({{
        '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;'
      }}[c]));
    }}

    function render() {{
      const type = typeFilter.value;
      const level = levelFilter.value;
      const target = targetFilter.value;
      const search = searchFilter.value.trim().toLowerCase();
      const filtered = entries.filter(entry => {{
        if (type && entry.type !== type) return false;
        if (level && entry.level !== level) return false;
        if (target && entry.target !== target) return false;
        if (search) {{
          const haystack = `${{entry.timestamp}} ${{entry.level}} ${{entry.type}} ${{entry.target}} ${{entry.message}} ${{entry.fields}} ${{entry.source}}`.toLowerCase();
          if (!haystack.includes(search)) return false;
        }}
        return true;
      }});
      rows.innerHTML = filtered.map(entry => `
        <tr>
          <td class="col-time">${{escapeHtml(entry.timestamp)}}</td>
          <td class="col-level level-${{escapeHtml(entry.level)}}">${{escapeHtml(entry.level)}}</td>
          <td class="col-type">${{escapeHtml(entry.type)}}</td>
          <td>${{escapeHtml(entry.target)}}</td>
          <td><div class="message">${{escapeHtml(entry.message)}}</div><div class="fields">${{escapeHtml(entry.fields)}}</div></td>
          <td class="col-source">${{escapeHtml(entry.source)}}</td>
        </tr>
      `).join('');
    }}

    [typeFilter, levelFilter, targetFilter, searchFilter].forEach(input => input.addEventListener('input', render));
    render();
  </script>
</body>
</html>"#,
    ))
}

fn entries_json(entries: &[LogEntry]) -> Vec<HashMap<&'static str, String>> {
    entries
        .iter()
        .map(|entry| {
            HashMap::from([
                ("source", entry.source.clone()),
                ("type", entry.log_type.clone()),
                ("level", entry.level.clone()),
                ("target", entry.target.clone()),
                ("timestamp", entry.timestamp.clone()),
                ("message", entry.message.clone()),
                ("fields", entry.fields.clone()),
                ("raw", entry.raw.clone()),
            ])
        })
        .collect()
}

fn unique_values<'a>(values: impl Iterator<Item = &'a str>) -> Vec<String> {
    values
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

fn counts_by_type(entries: &[LogEntry]) -> HashMap<String, usize> {
    let mut counts = HashMap::new();
    for entry in entries {
        *counts.entry(entry.log_type.clone()).or_insert(0) += 1;
    }
    counts
}

fn render_options(values: &[String]) -> String {
    values
        .iter()
        .map(|value| {
            format!(
                "<option value=\"{}\">{}</option>",
                html_escape(value),
                html_escape(value)
            )
        })
        .collect::<Vec<_>>()
        .join("")
}

fn render_summary(total: usize, counts: &HashMap<String, usize>) -> String {
    let application = counts.get("application").copied().unwrap_or(0);
    let error = counts.get("error").copied().unwrap_or(0);
    let performance = counts.get("performance").copied().unwrap_or(0);
    format!(
        r#"<div class="summary">
      <div class="metric"><div class="label">Total</div><div class="value">{total}</div></div>
      <div class="metric"><div class="label">Application</div><div class="value">{application}</div></div>
      <div class="metric"><div class="label">Error</div><div class="value">{error}</div></div>
      <div class="metric"><div class="label">Performance</div><div class="value">{performance}</div></div>
    </div>"#
    )
}

fn html_escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn print_usage() {
    println!("{}", usage());
}

fn usage() -> String {
    "usage: cargo run --bin log_report -- [--log-dir logs] [--output logs/log-report.html]"
        .to_string()
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::{classify_log_type, load_logs, parse_log_line, render_html};

    #[test]
    fn parse_json_log_line_extracts_message_and_fields() {
        let entry = parse_log_line(
            "application.log",
            "application",
            r#"{"timestamp":"2026-05-12T00:00:00Z","level":"INFO","target":"dao_game::world","fields":{"message":"world ready","chunks":4}}"#,
        );

        assert_eq!(entry.level, "INFO");
        assert_eq!(entry.target, "dao_game::world");
        assert_eq!(entry.message, "world ready");
        assert!(entry.fields.contains("chunks"));
    }

    #[test]
    fn load_logs_reads_current_and_rotated_files() {
        let temp_dir = tempfile::tempdir().expect("tempdir should exist");
        fs::write(
            temp_dir.path().join("application.log"),
            r#"{"level":"INFO","target":"dao_game::bootstrap","fields":{"message":"current"}}"#,
        )
        .unwrap();
        fs::write(
            temp_dir.path().join("application.log.1"),
            r#"{"level":"WARN","target":"dao_game::bootstrap","fields":{"message":"previous"}}"#,
        )
        .unwrap();

        let entries = load_logs(temp_dir.path()).unwrap();

        assert_eq!(entries.len(), 2);
        assert!(entries.iter().any(|entry| entry.message == "current"));
        assert!(entries.iter().any(|entry| entry.message == "previous"));
    }

    #[test]
    fn render_html_contains_filter_controls() {
        let entry = parse_log_line(
            "performance.log",
            "performance",
            r#"{"level":"WARN","target":"dao_game::performance::budget","fields":{"message":"frame budget exceeded"}}"#,
        );

        let html = render_html(&[entry]).unwrap();

        assert!(html.contains("typeFilter"));
        assert!(html.contains("levelFilter"));
        assert!(html.contains("dao_game::performance::budget"));
    }

    #[test]
    fn classify_log_type_uses_file_family() {
        assert_eq!(classify_log_type("performance.log.1"), "performance");
        assert_eq!(classify_log_type("error.log"), "error");
        assert_eq!(classify_log_type("application.log"), "application");
    }
}
