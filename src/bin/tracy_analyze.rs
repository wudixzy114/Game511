use std::{
    cmp::Ordering,
    collections::{HashMap, HashSet},
    env,
    fs::File,
    io::{BufRead, BufReader},
    path::{Path, PathBuf},
};

const DEFAULT_TOP: usize = 12;
const DEFAULT_MIN_TOTAL_MS: f64 = 0.05;

fn main() {
    if let Err(error) = run() {
        eprintln!("{error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let command = Command::parse(&env::args().skip(1).collect::<Vec<_>>())?;
    match command {
        Command::Analyze(config) => {
            let mut analysis = load_analysis(&config)?;
            print_report(&mut analysis, &config);
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
    Analyze(Config),
    Help,
}

#[derive(Debug, Clone)]
struct Config {
    summary: Option<PathBuf>,
    self_summary: Option<PathBuf>,
    events: Option<PathBuf>,
    top: usize,
    min_total_ms: f64,
}

impl Command {
    fn parse(args: &[String]) -> Result<Self, String> {
        if args.is_empty() {
            return Err(usage_error());
        }

        let mut config = Config {
            summary: None,
            self_summary: None,
            events: None,
            top: DEFAULT_TOP,
            min_total_ms: DEFAULT_MIN_TOTAL_MS,
        };
        let mut positional = Vec::new();
        let mut index = 0;
        while index < args.len() {
            match args[index].as_str() {
                "-h" | "--help" | "help" => return Ok(Self::Help),
                "--summary" => {
                    index += 1;
                    config.summary = Some(next_path(args, index, "--summary")?);
                }
                "--self" => {
                    index += 1;
                    config.self_summary = Some(next_path(args, index, "--self")?);
                }
                "--events" => {
                    index += 1;
                    config.events = Some(next_path(args, index, "--events")?);
                }
                "--top" => {
                    index += 1;
                    let value = args
                        .get(index)
                        .ok_or_else(|| "missing value for --top".to_string())?;
                    config.top = value
                        .parse::<usize>()
                        .map_err(|_| format!("invalid --top value: {value}"))?
                        .max(1);
                }
                "--min-total-ms" => {
                    index += 1;
                    let value = args
                        .get(index)
                        .ok_or_else(|| "missing value for --min-total-ms".to_string())?;
                    config.min_total_ms = value
                        .parse::<f64>()
                        .map_err(|_| format!("invalid --min-total-ms value: {value}"))?
                        .max(0.0);
                }
                value if value.starts_with("--") => {
                    return Err(format!("unknown option: {value}\n{}", usage_error()));
                }
                value => positional.push(PathBuf::from(value)),
            }
            index += 1;
        }

        if config.summary.is_none() {
            config.summary = positional.first().cloned();
        }
        if config.events.is_none() {
            config.events = positional.get(1).cloned();
        }
        if config.self_summary.is_none() {
            config.self_summary = positional.get(2).cloned();
        }

        if config.summary.is_none() && config.self_summary.is_none() && config.events.is_none() {
            return Err(usage_error());
        }

        Ok(Self::Analyze(config))
    }
}

fn next_path(args: &[String], index: usize, option: &str) -> Result<PathBuf, String> {
    args.get(index)
        .map(PathBuf::from)
        .ok_or_else(|| format!("missing value for {option}"))
}

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
struct ZoneKey {
    name: String,
    src_file: String,
    src_line: u32,
}

impl ZoneKey {
    fn new(name: &str, src_file: &str, src_line: u32) -> Self {
        Self {
            name: normalize_zone_name(name),
            src_file: src_file.trim().to_string(),
            src_line,
        }
    }
}

#[derive(Debug, Default, Clone)]
struct SummaryAgg {
    total_ns: f64,
    total_perc: f64,
    count: u64,
    mean_ns: f64,
    min_ns: f64,
    max_ns: f64,
    std_ns: f64,
}

impl SummaryAgg {
    fn add(&mut self, row: SummaryRow) {
        if self.count == 0 {
            self.min_ns = row.min_ns;
        } else {
            self.min_ns = self.min_ns.min(row.min_ns);
        }
        self.total_ns += row.total_ns;
        self.total_perc += row.total_perc;
        self.count += row.count;
        self.max_ns = self.max_ns.max(row.max_ns);
        self.std_ns = self.std_ns.max(row.std_ns);
        self.mean_ns = if self.count > 0 {
            self.total_ns / self.count as f64
        } else {
            row.mean_ns
        };
    }
}

#[derive(Debug, Clone, Copy)]
struct SummaryRow {
    total_ns: f64,
    total_perc: f64,
    count: u64,
    mean_ns: f64,
    min_ns: f64,
    max_ns: f64,
    std_ns: f64,
}

#[derive(Debug, Default, Clone)]
struct EventAgg {
    durations_ns: Vec<u64>,
    total_ns: u128,
    count: u64,
    min_ns: u64,
    max_ns: u64,
    thread_totals: HashMap<String, ThreadAgg>,
}

impl EventAgg {
    fn add(&mut self, start_ns: u64, duration_ns: u64, thread: &str, overview: &mut EventOverview) {
        if self.count == 0 {
            self.min_ns = duration_ns;
        } else {
            self.min_ns = self.min_ns.min(duration_ns);
        }
        self.count += 1;
        self.total_ns += duration_ns as u128;
        self.max_ns = self.max_ns.max(duration_ns);
        self.durations_ns.push(duration_ns);
        self.thread_totals
            .entry(thread.to_string())
            .or_default()
            .add(duration_ns);
        overview.add(start_ns, duration_ns, thread);
    }
}

#[derive(Debug, Default, Clone)]
struct ThreadAgg {
    total_ns: u128,
    count: u64,
    max_ns: u64,
}

impl ThreadAgg {
    fn add(&mut self, duration_ns: u64) {
        self.total_ns += duration_ns as u128;
        self.count += 1;
        self.max_ns = self.max_ns.max(duration_ns);
    }
}

#[derive(Debug, Default, Clone)]
struct EventOverview {
    first_start_ns: Option<u64>,
    last_end_ns: u64,
    total_events: u64,
    thread_totals: HashMap<String, ThreadAgg>,
}

impl EventOverview {
    fn add(&mut self, start_ns: u64, duration_ns: u64, thread: &str) {
        self.first_start_ns = Some(
            self.first_start_ns
                .map_or(start_ns, |first| first.min(start_ns)),
        );
        self.last_end_ns = self.last_end_ns.max(start_ns.saturating_add(duration_ns));
        self.total_events += 1;
        self.thread_totals
            .entry(thread.to_string())
            .or_default()
            .add(duration_ns);
    }

    fn duration_s(&self) -> Option<f64> {
        self.first_start_ns
            .map(|first| ns_to_s(self.last_end_ns.saturating_sub(first) as f64))
            .filter(|duration| *duration > 0.0)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum SourceKind {
    Project,
    Dependency,
    External,
    Unknown,
}

impl SourceKind {
    fn label(self) -> &'static str {
        match self {
            Self::Project => "project",
            Self::Dependency => "dependency",
            Self::External => "external",
            Self::Unknown => "unknown",
        }
    }
}

#[derive(Debug, Clone)]
struct Analysis {
    summary_path: Option<PathBuf>,
    self_path: Option<PathBuf>,
    event_path: Option<PathBuf>,
    zones: Vec<ZoneAnalysis>,
    event_overview: EventOverview,
    total_inclusive_ns: f64,
    total_self_ns: f64,
    source_totals: HashMap<SourceKind, f64>,
}

#[derive(Debug, Clone)]
struct ZoneAnalysis {
    key: ZoneKey,
    kind: SourceKind,
    inclusive: Option<SummaryAgg>,
    self_summary: Option<SummaryAgg>,
    event_stats: Option<EventStats>,
    score: f64,
}

#[derive(Debug, Clone)]
struct EventStats {
    count: u64,
    total_ns: f64,
    mean_ns: f64,
    min_ns: f64,
    p50_ns: f64,
    p90_ns: f64,
    p95_ns: f64,
    p99_ns: f64,
    max_ns: f64,
    thread_count: usize,
    top_thread: Option<(String, ThreadAgg)>,
}

fn load_analysis(config: &Config) -> Result<Analysis, String> {
    let inclusive = match &config.summary {
        Some(path) => load_summary_csv(path)?,
        None => HashMap::new(),
    };
    let self_summary = match &config.self_summary {
        Some(path) => load_summary_csv(path)?,
        None => HashMap::new(),
    };
    let (mut events, event_overview) = match &config.events {
        Some(path) => load_event_csv(path)?,
        None => (HashMap::new(), EventOverview::default()),
    };

    let mut keys: HashSet<ZoneKey> = inclusive.keys().cloned().collect();
    keys.extend(self_summary.keys().cloned());
    keys.extend(events.keys().cloned());

    let total_inclusive_ns = if inclusive.is_empty() {
        events.values().map(|event| event.total_ns as f64).sum()
    } else {
        inclusive.values().map(|summary| summary.total_ns).sum()
    };
    let total_self_ns = if self_summary.is_empty() {
        total_inclusive_ns
    } else {
        self_summary.values().map(|summary| summary.total_ns).sum()
    };

    let current_dir = env::current_dir().ok();
    let mut zones = Vec::new();
    let mut source_totals = HashMap::new();

    for key in keys {
        let event_stats = events.get_mut(&key).map(event_stats);
        let inclusive_summary = inclusive.get(&key).cloned();
        let self_zone_summary = self_summary.get(&key).cloned();
        let kind = source_kind(&key.src_file, current_dir.as_deref());
        let score = heuristic_score(
            &inclusive_summary,
            &self_zone_summary,
            event_stats.as_ref(),
            kind,
            total_self_ns,
        );
        let contribution_ns = self_zone_summary
            .as_ref()
            .map(|summary| summary.total_ns)
            .or_else(|| inclusive_summary.as_ref().map(|summary| summary.total_ns))
            .or_else(|| event_stats.as_ref().map(|stats| stats.total_ns))
            .unwrap_or(0.0);
        *source_totals.entry(kind).or_insert(0.0) += contribution_ns;
        zones.push(ZoneAnalysis {
            key,
            kind,
            inclusive: inclusive_summary,
            self_summary: self_zone_summary,
            event_stats,
            score,
        });
    }

    zones.sort_by(|left, right| compare_f64_desc(left.score, right.score));

    Ok(Analysis {
        summary_path: config.summary.clone(),
        self_path: config.self_summary.clone(),
        event_path: config.events.clone(),
        zones,
        event_overview,
        total_inclusive_ns,
        total_self_ns,
        source_totals,
    })
}

fn load_summary_csv(path: &Path) -> Result<HashMap<ZoneKey, SummaryAgg>, String> {
    let file =
        File::open(path).map_err(|error| format!("failed to open {}: {error}", path.display()))?;
    let mut lines = BufReader::new(file).lines();
    let header = next_nonempty_line(&mut lines, path)?;
    let separator = detect_separator(&header);
    let columns = split_raw(&header, separator);
    require_columns(
        &columns,
        &[
            "name",
            "src_file",
            "src_line",
            "total_ns",
            "total_perc",
            "counts",
            "mean_ns",
            "min_ns",
            "max_ns",
            "std_ns",
        ],
        path,
    )?;

    let mut rows = HashMap::new();
    for line in lines {
        let line = line.map_err(|error| format!("failed to read {}: {error}", path.display()))?;
        if line.trim().is_empty() {
            continue;
        }
        let fields = split_record(&line, separator, columns.len());
        if fields.len() < columns.len() {
            continue;
        }
        let name = field(&fields, &columns, "name")?;
        let src_file = field(&fields, &columns, "src_file")?;
        let src_line = parse_u32(field(&fields, &columns, "src_line")?);
        let row = SummaryRow {
            total_ns: parse_f64(field(&fields, &columns, "total_ns")?),
            total_perc: parse_f64(field(&fields, &columns, "total_perc")?),
            count: parse_u64(field(&fields, &columns, "counts")?),
            mean_ns: parse_f64(field(&fields, &columns, "mean_ns")?),
            min_ns: parse_f64(field(&fields, &columns, "min_ns")?),
            max_ns: parse_f64(field(&fields, &columns, "max_ns")?),
            std_ns: parse_f64(field(&fields, &columns, "std_ns")?),
        };
        rows.entry(ZoneKey::new(name, src_file, src_line))
            .or_insert_with(SummaryAgg::default)
            .add(row);
    }
    Ok(rows)
}

fn load_event_csv(path: &Path) -> Result<(HashMap<ZoneKey, EventAgg>, EventOverview), String> {
    let file =
        File::open(path).map_err(|error| format!("failed to open {}: {error}", path.display()))?;
    let mut lines = BufReader::new(file).lines();
    let header = next_nonempty_line(&mut lines, path)?;
    let separator = detect_separator(&header);
    let columns = split_raw(&header, separator);
    require_columns(
        &columns,
        &[
            "name",
            "src_file",
            "src_line",
            "ns_since_start",
            "exec_time_ns",
            "thread",
        ],
        path,
    )?;

    let mut rows = HashMap::new();
    let mut overview = EventOverview::default();
    for line in lines {
        let line = line.map_err(|error| format!("failed to read {}: {error}", path.display()))?;
        if line.trim().is_empty() {
            continue;
        }
        let fields = split_record(&line, separator, columns.len());
        if fields.len() < columns.len() {
            continue;
        }
        let name = field(&fields, &columns, "name")?;
        let src_file = field(&fields, &columns, "src_file")?;
        let src_line = parse_u32(field(&fields, &columns, "src_line")?);
        let start_ns = parse_u64(field(&fields, &columns, "ns_since_start")?);
        let duration_ns = parse_u64(field(&fields, &columns, "exec_time_ns")?);
        let thread = field(&fields, &columns, "thread")?;
        rows.entry(ZoneKey::new(name, src_file, src_line))
            .or_insert_with(EventAgg::default)
            .add(start_ns, duration_ns, thread, &mut overview);
    }
    Ok((rows, overview))
}

fn next_nonempty_line(
    lines: &mut impl Iterator<Item = Result<String, std::io::Error>>,
    path: &Path,
) -> Result<String, String> {
    for line in lines {
        let line = line.map_err(|error| format!("failed to read {}: {error}", path.display()))?;
        let line = line.trim_start_matches('\u{feff}').to_string();
        if !line.trim().is_empty() {
            return Ok(line);
        }
    }
    Err(format!("empty CSV: {}", path.display()))
}

fn require_columns(columns: &[String], required: &[&str], path: &Path) -> Result<(), String> {
    for required_column in required {
        if !columns.iter().any(|column| column == required_column) {
            return Err(format!(
                "{} is missing required column `{required_column}`",
                path.display()
            ));
        }
    }
    Ok(())
}

fn field<'a>(fields: &'a [String], columns: &[String], column: &str) -> Result<&'a str, String> {
    let index = columns
        .iter()
        .position(|candidate| candidate == column)
        .ok_or_else(|| format!("missing column: {column}"))?;
    fields
        .get(index)
        .map(|value| value.trim())
        .ok_or_else(|| format!("missing value for column: {column}"))
}

fn detect_separator(header: &str) -> char {
    let semicolons = header.matches(';').count();
    let commas = header.matches(',').count();
    if semicolons >= commas { ';' } else { ',' }
}

fn split_raw(line: &str, separator: char) -> Vec<String> {
    line.trim_start_matches('\u{feff}')
        .split(separator)
        .map(|field| field.trim().to_string())
        .collect()
}

fn split_record(line: &str, separator: char, expected_columns: usize) -> Vec<String> {
    let mut parts = split_raw(line, separator);
    if expected_columns == 0 || parts.len() <= expected_columns {
        return parts;
    }

    let extra_name_fields = parts.len() - expected_columns;
    let merged_name = parts
        .drain(0..=extra_name_fields)
        .collect::<Vec<_>>()
        .join(&separator.to_string());
    let mut normalized = Vec::with_capacity(expected_columns);
    normalized.push(merged_name);
    normalized.extend(parts);
    normalized
}

fn normalize_zone_name(name: &str) -> String {
    let trimmed = name.trim();
    if let Some(index) = trimmed.find('{') {
        trimmed[..index].trim_end().to_string()
    } else {
        trimmed.to_string()
    }
}

fn parse_f64(value: &str) -> f64 {
    value.trim().parse::<f64>().unwrap_or(0.0)
}

fn parse_u64(value: &str) -> u64 {
    value.trim().parse::<u64>().unwrap_or(0)
}

fn parse_u32(value: &str) -> u32 {
    value.trim().parse::<u32>().unwrap_or(0)
}

fn event_stats(agg: &mut EventAgg) -> EventStats {
    agg.durations_ns.sort_unstable();
    let count = agg.count.max(1);
    let top_thread = agg
        .thread_totals
        .iter()
        .max_by(|(_, left), (_, right)| left.total_ns.cmp(&right.total_ns))
        .map(|(thread, agg)| (thread.clone(), agg.clone()));

    EventStats {
        count: agg.count,
        total_ns: agg.total_ns as f64,
        mean_ns: agg.total_ns as f64 / count as f64,
        min_ns: agg.min_ns as f64,
        p50_ns: percentile(&agg.durations_ns, 0.50),
        p90_ns: percentile(&agg.durations_ns, 0.90),
        p95_ns: percentile(&agg.durations_ns, 0.95),
        p99_ns: percentile(&agg.durations_ns, 0.99),
        max_ns: agg.max_ns as f64,
        thread_count: agg.thread_totals.len(),
        top_thread,
    }
}

fn percentile(sorted: &[u64], percentile: f64) -> f64 {
    if sorted.is_empty() {
        return 0.0;
    }
    if sorted.len() == 1 {
        return sorted[0] as f64;
    }
    let rank = percentile.clamp(0.0, 1.0) * (sorted.len() - 1) as f64;
    let lower = rank.floor() as usize;
    let upper = rank.ceil() as usize;
    if lower == upper {
        sorted[lower] as f64
    } else {
        let ratio = rank - lower as f64;
        sorted[lower] as f64 * (1.0 - ratio) + sorted[upper] as f64 * ratio
    }
}

fn heuristic_score(
    inclusive: &Option<SummaryAgg>,
    self_summary: &Option<SummaryAgg>,
    events: Option<&EventStats>,
    kind: SourceKind,
    total_self_ns: f64,
) -> f64 {
    let inclusive_ms = inclusive
        .as_ref()
        .map(|summary| ns_to_ms(summary.total_ns))
        .or_else(|| events.map(|stats| ns_to_ms(stats.total_ns)))
        .unwrap_or(0.0);
    let self_ms = self_summary
        .as_ref()
        .map(|summary| ns_to_ms(summary.total_ns))
        .unwrap_or(inclusive_ms);
    let p99_ms = events.map(|stats| ns_to_ms(stats.p99_ns)).unwrap_or(0.0);
    let max_ms = events
        .map(|stats| ns_to_ms(stats.max_ns))
        .or_else(|| inclusive.as_ref().map(|summary| ns_to_ms(summary.max_ns)))
        .unwrap_or(0.0);
    let count = self_summary
        .as_ref()
        .map(|summary| summary.count)
        .or_else(|| inclusive.as_ref().map(|summary| summary.count))
        .or_else(|| events.map(|stats| stats.count))
        .unwrap_or(0);
    let share_boost = if total_self_ns > 0.0 {
        (self_summary
            .as_ref()
            .map(|summary| summary.total_ns)
            .unwrap_or(0.0)
            / total_self_ns)
            * 100.0
    } else {
        0.0
    };
    let frequency_boost = if count > 10_000 {
        (count as f64).log10()
    } else {
        0.0
    };
    let source_boost = match kind {
        SourceKind::Project => 1.15,
        SourceKind::Dependency => 1.0,
        SourceKind::External => 0.9,
        SourceKind::Unknown => 0.85,
    };

    (self_ms * 0.80
        + inclusive_ms * 0.20
        + p99_ms * 5.0
        + max_ms * 0.8
        + share_boost * 8.0
        + frequency_boost)
        * source_boost
}

fn source_kind(src_file: &str, current_dir: Option<&Path>) -> SourceKind {
    if src_file.trim().is_empty() {
        return SourceKind::Unknown;
    }
    let normalized = normalize_path_text(src_file);
    if let Some(current_dir) = current_dir {
        let current = normalize_path_text(&current_dir.display().to_string());
        if normalized.starts_with(&current) {
            return SourceKind::Project;
        }
    }
    if normalized.contains("\\.cargo\\registry\\") || normalized.contains("\\.cargo\\git\\") {
        SourceKind::Dependency
    } else {
        SourceKind::External
    }
}

fn normalize_path_text(path: &str) -> String {
    path.replace('/', "\\").to_ascii_lowercase()
}

fn print_report(analysis: &mut Analysis, config: &Config) {
    println!("Tracy heuristic analysis");
    println!(
        "summary: {}",
        display_optional_path(analysis.summary_path.as_deref())
    );
    println!(
        "self:    {}",
        display_optional_path(analysis.self_path.as_deref())
    );
    println!(
        "events:  {}",
        display_optional_path(analysis.event_path.as_deref())
    );
    println!();

    print_overview(analysis);
    print_ranked_zones(analysis, config);
    print_thread_load(analysis);
}

fn print_overview(analysis: &Analysis) {
    let duration = analysis
        .event_overview
        .duration_s()
        .map(|duration| format!("{duration:.2} s"))
        .unwrap_or_else(|| "n/a".to_string());
    println!("Overview");
    println!(
        "  trace window: {duration}; zones: {}; event samples: {}; inclusive zone time: {:.2} ms; self zone time: {:.2} ms",
        analysis.zones.len(),
        analysis.event_overview.total_events,
        ns_to_ms(analysis.total_inclusive_ns),
        ns_to_ms(analysis.total_self_ns),
    );
    println!("  source share: {}", source_share_text(analysis));
    println!();
}

fn source_share_text(analysis: &Analysis) -> String {
    let total: f64 = analysis.source_totals.values().sum();
    if total <= f64::EPSILON {
        return "n/a".to_string();
    }
    [
        SourceKind::Project,
        SourceKind::Dependency,
        SourceKind::External,
        SourceKind::Unknown,
    ]
    .iter()
    .filter_map(|kind| {
        let value = analysis.source_totals.get(kind).copied().unwrap_or(0.0);
        if value <= f64::EPSILON {
            None
        } else {
            Some(format!("{} {:.1}%", kind.label(), value * 100.0 / total))
        }
    })
    .collect::<Vec<_>>()
    .join(", ")
}

fn print_ranked_zones(analysis: &Analysis, config: &Config) {
    println!("Most useful zones");
    let mut printed = 0;
    for zone in analysis.zones.iter().filter(|zone| {
        zone_total_ms(zone) >= config.min_total_ms || zone_event_max_ms(zone) >= config.min_total_ms
    }) {
        if printed >= config.top {
            break;
        }
        printed += 1;
        let reasons = finding_reasons(zone, analysis);
        println!(
            "{:>2}. {} [{}]",
            printed,
            truncate_middle(&zone.key.name, 88),
            zone.kind.label()
        );
        println!(
            "    total {:>9.2} ms ({:>5.1}%), self {:>9}, calls {:>7}, mean {:>8}, p95 {:>8}, p99 {:>8}, max {:>8}",
            zone_total_ms(zone),
            zone_total_percent(zone, analysis),
            optional_ms(zone_self_ns(zone)),
            zone_call_count(zone),
            optional_ms(zone_mean_ns(zone)),
            optional_ms(zone.event_stats.as_ref().map(|stats| stats.p95_ns)),
            optional_ms(zone.event_stats.as_ref().map(|stats| stats.p99_ns)),
            optional_ms(zone_max_ns(zone)),
        );
        println!(
            "    source {}",
            compact_source(&zone.key.src_file, zone.key.src_line)
        );
        if let Some(stats) = &zone.event_stats {
            println!(
                "    event dist min {}, p50 {}, p90 {}",
                optional_ms(Some(stats.min_ns)),
                optional_ms(Some(stats.p50_ns)),
                optional_ms(Some(stats.p90_ns)),
            );
            if let Some((thread, thread_agg)) = &stats.top_thread {
                println!(
                    "    events threads {}; top thread {} {:.2} ms / {} events",
                    stats.thread_count,
                    thread,
                    ns_to_ms(thread_agg.total_ns as f64),
                    thread_agg.count
                );
            }
        }
        if !reasons.is_empty() {
            println!("    why: {}", reasons.join("; "));
        }
    }
    if printed == 0 {
        println!(
            "  no zones passed --min-total-ms {:.2}",
            config.min_total_ms
        );
    }
    println!();
}

fn print_thread_load(analysis: &Analysis) {
    if analysis.event_overview.thread_totals.is_empty() {
        return;
    }
    println!("Thread event-time load");
    let mut threads: Vec<_> = analysis.event_overview.thread_totals.iter().collect();
    threads.sort_by(|(_, left), (_, right)| right.total_ns.cmp(&left.total_ns));
    for (thread, agg) in threads.into_iter().take(6) {
        println!(
            "  thread {:>4}: {:>9.2} ms across {:>7} events; max event {:>8}",
            thread,
            ns_to_ms(agg.total_ns as f64),
            agg.count,
            optional_ms(Some(agg.max_ns as f64)),
        );
    }
}

fn finding_reasons(zone: &ZoneAnalysis, analysis: &Analysis) -> Vec<String> {
    let mut reasons = Vec::new();
    let self_ns = zone_self_ns(zone).unwrap_or_else(|| zone_total_ms(zone) * 1_000_000.0);
    let self_share = if analysis.total_self_ns > f64::EPSILON {
        self_ns * 100.0 / analysis.total_self_ns
    } else {
        0.0
    };
    let total_ms = zone_total_ms(zone);
    let self_ms = ns_to_ms(self_ns);
    if self_share >= 15.0 {
        reasons.push(format!(
            "dominant self hotspot ({self_share:.1}% of self time)"
        ));
    } else if zone_total_percent(zone, analysis) >= 15.0 && self_ms < total_ms * 0.35 {
        reasons
            .push("large parent/wrapper zone; inspect child zones before editing it".to_string());
    }

    if let Some(stats) = &zone.event_stats {
        let mean_ms = ns_to_ms(stats.mean_ns);
        let p99_ms = ns_to_ms(stats.p99_ns);
        let max_ms = ns_to_ms(stats.max_ns);
        if p99_ms >= 2.0 {
            reasons.push(format!("high p99 event time ({p99_ms:.2} ms)"));
        }
        if mean_ms > 0.0 && max_ms >= 4.0 && max_ms / mean_ms >= 4.0 {
            reasons.push(format!("spiky outlier (max/mean {:.1}x)", max_ms / mean_ms));
        }
        if let Some(duration_s) = analysis.event_overview.duration_s() {
            let calls_per_s = stats.count as f64 / duration_s;
            if calls_per_s >= 1_000.0 && mean_ms <= 0.05 && total_ms >= 5.0 {
                reasons.push(format!(
                    "very frequent tiny zone ({calls_per_s:.0} calls/s)"
                ));
            }
        }
    }

    if reasons.is_empty() {
        reasons
            .push("ranked by combined self time, total time, p99, max, and call count".to_string());
    }
    reasons
}

fn zone_total_ms(zone: &ZoneAnalysis) -> f64 {
    zone.inclusive
        .as_ref()
        .map(|summary| ns_to_ms(summary.total_ns))
        .or_else(|| {
            zone.event_stats
                .as_ref()
                .map(|stats| ns_to_ms(stats.total_ns))
        })
        .unwrap_or(0.0)
}

fn zone_total_percent(zone: &ZoneAnalysis, analysis: &Analysis) -> f64 {
    zone.inclusive
        .as_ref()
        .map(|summary| summary.total_perc)
        .unwrap_or_else(|| {
            if analysis.total_inclusive_ns > f64::EPSILON {
                zone_total_ms(zone) * 1_000_000.0 * 100.0 / analysis.total_inclusive_ns
            } else {
                0.0
            }
        })
}

fn zone_self_ns(zone: &ZoneAnalysis) -> Option<f64> {
    zone.self_summary.as_ref().map(|summary| summary.total_ns)
}

fn zone_mean_ns(zone: &ZoneAnalysis) -> Option<f64> {
    zone.event_stats
        .as_ref()
        .map(|stats| stats.mean_ns)
        .or_else(|| zone.inclusive.as_ref().map(|summary| summary.mean_ns))
}

fn zone_max_ns(zone: &ZoneAnalysis) -> Option<f64> {
    zone.event_stats
        .as_ref()
        .map(|stats| stats.max_ns)
        .or_else(|| zone.inclusive.as_ref().map(|summary| summary.max_ns))
}

fn zone_event_max_ms(zone: &ZoneAnalysis) -> f64 {
    zone.event_stats
        .as_ref()
        .map(|stats| ns_to_ms(stats.max_ns))
        .unwrap_or(0.0)
}

fn zone_call_count(zone: &ZoneAnalysis) -> u64 {
    zone.event_stats
        .as_ref()
        .map(|stats| stats.count)
        .or_else(|| zone.inclusive.as_ref().map(|summary| summary.count))
        .or_else(|| zone.self_summary.as_ref().map(|summary| summary.count))
        .unwrap_or(0)
}

fn optional_ms(ns: Option<f64>) -> String {
    ns.map(|value| format!("{:.2} ms", ns_to_ms(value)))
        .unwrap_or_else(|| "n/a".to_string())
}

fn ns_to_ms(ns: f64) -> f64 {
    ns / 1_000_000.0
}

fn ns_to_s(ns: f64) -> f64 {
    ns / 1_000_000_000.0
}

fn compare_f64_desc(left: f64, right: f64) -> Ordering {
    right.partial_cmp(&left).unwrap_or(Ordering::Equal)
}

fn display_optional_path(path: Option<&Path>) -> String {
    path.map(|path| path.display().to_string())
        .unwrap_or_else(|| "n/a".to_string())
}

fn compact_source(src_file: &str, src_line: u32) -> String {
    let source = compact_path(src_file);
    if src_line > 0 {
        format!("{source}:{src_line}")
    } else {
        source
    }
}

fn compact_path(path: &str) -> String {
    if path.trim().is_empty() {
        return "unknown".to_string();
    }
    let normalized_lower = normalize_path_text(path);
    if let Ok(current_dir) = env::current_dir() {
        let current = normalize_path_text(&current_dir.display().to_string());
        if normalized_lower.starts_with(&current) {
            return path
                .get(current_dir.display().to_string().len()..)
                .unwrap_or(path)
                .trim_start_matches(['\\', '/'])
                .to_string();
        }
    }

    let marker = "\\.cargo\\registry\\src\\";
    if let Some(index) = normalized_lower.find(marker) {
        let tail = &path[index + marker.len()..];
        let mut parts = tail.split(['\\', '/']);
        let _registry_hash = parts.next();
        return parts.collect::<Vec<_>>().join("\\");
    }
    path.to_string()
}

fn truncate_middle(value: &str, max_chars: usize) -> String {
    let char_count = value.chars().count();
    if char_count <= max_chars {
        return value.to_string();
    }
    let head = max_chars.saturating_sub(3) / 2;
    let tail = max_chars.saturating_sub(3) - head;
    let start: String = value.chars().take(head).collect();
    let end: String = value
        .chars()
        .rev()
        .take(tail)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    format!("{start}...{end}")
}

fn print_usage() {
    println!("{}", usage_error());
}

fn usage_error() -> String {
    "usage:
  cargo run --bin tracy_analyze -- --summary <summary.csv> [--self <self.csv>] [--events <events.csv>]
  cargo run --bin tracy_analyze -- <summary.csv> [events.csv] [self.csv]
options:
  --top <n>             number of ranked zones to print (default: 12)
  --min-total-ms <ms>   hide zones below this total/max threshold (default: 0.05)

Expected CSV files should come from tracy-csvexport, preferably with -s ';':
  tracy-csvexport.exe -s ';' trace.tracy
  tracy-csvexport.exe -e -s ';' trace.tracy
  tracy-csvexport.exe -u -s ';' trace.tracy"
        .to_string()
}

#[cfg(test)]
mod tests {
    use std::{
        fs,
        path::PathBuf,
        time::{SystemTime, UNIX_EPOCH},
    };

    use super::{Config, load_analysis, load_summary_csv, normalize_zone_name, percentile};

    #[test]
    fn normalizes_tracing_field_suffixes() {
        assert_eq!(
            normalize_zone_name("winit::Window::set_visible{visible=true}"),
            "winit::Window::set_visible"
        );
    }

    #[test]
    fn loads_semicolon_summary_and_merges_normalized_names() {
        let path = unique_temp_path("tracy-summary", "csv");
        fs::write(
            &path,
            concat!(
                "name;src_file;src_line;total_ns;total_perc;counts;mean_ns;min_ns;max_ns;std_ns\n",
                "foo{a=1};D:\\Game511\\src\\foo.rs;10;1000;10.0;2;500;100;900;10\n",
                "foo{a=2};D:\\Game511\\src\\foo.rs;10;3000;30.0;3;1000;100;2000;20\n"
            ),
        )
        .unwrap();

        let rows = load_summary_csv(&path).unwrap();
        let row = rows.values().next().unwrap();

        assert_eq!(rows.len(), 1);
        assert_eq!(row.total_ns, 4000.0);
        assert_eq!(row.count, 5);
        assert_eq!(row.max_ns, 2000.0);

        let _ = fs::remove_file(path);
    }

    #[test]
    fn analysis_uses_events_for_percentiles() {
        let summary = unique_temp_path("tracy-summary", "csv");
        let events = unique_temp_path("tracy-events", "csv");
        fs::write(
            &summary,
            concat!(
                "name;src_file;src_line;total_ns;total_perc;counts;mean_ns;min_ns;max_ns;std_ns\n",
                "foo;D:\\Game511\\src\\foo.rs;10;10000000;100.0;3;3333333;1000000;7000000;1\n"
            ),
        )
        .unwrap();
        fs::write(
            &events,
            concat!(
                "name;src_file;src_line;ns_since_start;exec_time_ns;thread;value\n",
                "foo;D:\\Game511\\src\\foo.rs;10;0;1000000;1;\n",
                "foo;D:\\Game511\\src\\foo.rs;10;2000000;2000000;1;\n",
                "foo;D:\\Game511\\src\\foo.rs;10;5000000;7000000;2;\n"
            ),
        )
        .unwrap();
        let config = Config {
            summary: Some(summary.clone()),
            self_summary: None,
            events: Some(events.clone()),
            top: 10,
            min_total_ms: 0.0,
        };

        let analysis = load_analysis(&config).unwrap();
        let stats = analysis.zones[0].event_stats.as_ref().unwrap();

        assert_eq!(stats.count, 3);
        assert_eq!(stats.thread_count, 2);
        assert!(stats.p95_ns > 6_000_000.0);

        let _ = fs::remove_file(summary);
        let _ = fs::remove_file(events);
    }

    #[test]
    fn percentile_interpolates() {
        assert_eq!(percentile(&[10, 20, 30], 0.5), 20.0);
        assert_eq!(percentile(&[10, 20], 0.5), 15.0);
    }

    fn unique_temp_path(prefix: &str, extension: &str) -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("{prefix}-{unique}.{extension}"))
    }
}
