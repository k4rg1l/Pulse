//! Claude subscription usage, from files Claude Code leaves on this machine.
//!
//! This source never reads the Claude login. Anthropic does not allow other apps
//! to use Claude.ai tokens, and there is no public usage endpoint, so it uses:
//!
//! - Limits: `claude_status_line` connects Claude Code's status line, which saves
//!   them to a file. That file only changes while Claude Code is responding.
//! - Tokens: `usage` numbers from the transcripts in `~/.claude/projects`. The
//!   conversation text in those files is never kept.

use std::{
    collections::{HashMap, HashSet},
    fs,
    path::{Path, PathBuf},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use async_trait::async_trait;
use serde::Deserialize;
use serde_json::{Value as Json, json};
use time::{OffsetDateTime, format_description::well_known::Rfc3339};

use super::{Metric, Reading, Setup, Source, Unit, Widget, WidgetType, claude_status_line};

const INTERVAL: Duration = Duration::from_secs(30);
const FIVE_HOURS: i64 = 5 * 60 * 60;
const SEVEN_DAYS: i64 = 7 * 24 * 60 * 60;

const MODELS_SHOWN: usize = 4;

#[derive(Default)]
pub struct ClaudeUsage;

#[async_trait]
impl Source for ClaudeUsage {
    fn id(&self) -> &'static str {
        "claude"
    }

    fn name(&self) -> &'static str {
        "Claude"
    }

    fn interval(&self) -> Duration {
        INTERVAL
    }

    fn widgets(&self) -> Vec<Widget> {
        vec![
            Widget {
                id: "five-hour-trend",
                kind: WidgetType::Chart,
                title: "5-hour limit used",
                keys: &["claude.five_hour.usage"],
            },
            Widget {
                id: "five-hour",
                kind: WidgetType::Meter,
                title: "5-hour limit",
                keys: &["claude.five_hour.usage", "claude.five_hour.resets_at"],
            },
            Widget {
                id: "tokens-five-hour",
                kind: WidgetType::Stat,
                title: "Tokens, 5h",
                keys: &["claude.tokens.five_hour"],
            },
            Widget {
                id: "seven-day",
                kind: WidgetType::Meter,
                title: "Weekly limit",
                keys: &["claude.seven_day.usage", "claude.seven_day.resets_at"],
            },
            Widget {
                id: "limits-as-of",
                kind: WidgetType::Stat,
                title: "Limits as of",
                keys: &["claude.limits_as_of"],
            },
            Widget {
                id: "models",
                kind: WidgetType::Details,
                title: "Tokens this week",
                keys: &["claude.models"],
            },
        ]
    }

    fn metrics(&self) -> Vec<Metric> {
        vec![
            Metric {
                key: "claude.five_hour.usage",
                label: "Claude 5-hour limit",
                unit: Unit::Percent,
            },
            Metric {
                key: "claude.seven_day.usage",
                label: "Claude weekly limit",
                unit: Unit::Percent,
            },
            Metric {
                key: "claude.tokens.five_hour",
                label: "Claude tokens, 5 hours",
                unit: Unit::Count,
            },
            Metric {
                key: "claude.tokens.seven_day",
                label: "Claude tokens, 7 days",
                unit: Unit::Count,
            },
            Metric {
                key: "claude.limits_as_of",
                label: "Claude limits as of",
                unit: Unit::Timestamp,
            },
        ]
    }

    fn setup(&self) -> Option<Setup> {
        Some(claude_status_line::setup())
    }

    fn connect(&self) -> Result<(), String> {
        claude_status_line::connect()
    }

    fn disconnect(&self) -> Result<(), String> {
        claude_status_line::disconnect()
    }

    async fn run(&self, _api_keys: HashMap<String, String>) -> Result<Vec<Reading>, String> {
        tokio::task::spawn_blocking(read_usage)
            .await
            .map_err(|e| format!("Reading Claude usage stopped: {e}"))?
    }
}

fn read_usage() -> Result<Vec<Reading>, String> {
    let now = now_seconds();
    let mut readings = read_limits(now);

    // Without Claude Code there is nothing to report. The tab's setup card says so.
    let projects = claude_status_line::claude_dir().map(|dir| dir.join("projects"));
    if let Some(projects) = projects.filter(|projects| projects.is_dir()) {
        let tally = tally_tokens(&projects, now);
        readings.push(Reading::new("claude.tokens.five_hour", tally.five_hour));
        readings.push(Reading::new("claude.tokens.seven_day", tally.seven_day));
        readings.push(Reading::current_only(
            "claude.models",
            tally.models_summary(),
        ));
    }
    Ok(readings)
}

fn now_seconds() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |since| since.as_secs() as i64)
}

#[derive(Deserialize)]
struct StatusFile {
    rate_limits: Option<RateLimits>,
}

#[derive(Deserialize)]
struct RateLimits {
    five_hour: Option<LimitWindow>,
    seven_day: Option<LimitWindow>,
}

#[derive(Deserialize)]
struct LimitWindow {
    used_percentage: f64,
    resets_at: i64,
}

/// Readings for both limit windows. With no status file they are cleared, so
/// that disconnecting leaves no old figures behind. A half-written file is
/// skipped until the next run.
fn read_limits(now: i64) -> Vec<Reading> {
    let Some(path) = claude_status_line::status_file().filter(|path| path.exists()) else {
        return [
            "claude.five_hour.usage",
            "claude.five_hour.resets_at",
            "claude.seven_day.usage",
            "claude.seven_day.resets_at",
            "claude.limits_as_of",
        ]
        .map(|key| Reading::current_only(key, Json::Null))
        .into();
    };

    let Some(limits) = fs::read(&path)
        .ok()
        .and_then(|bytes| serde_json::from_slice::<StatusFile>(&bytes).ok())
        .and_then(|status| status.rate_limits)
    else {
        return vec![];
    };

    let mut readings = window_readings("claude.five_hour", limits.five_hour, now);
    readings.extend(window_readings("claude.seven_day", limits.seven_day, now));

    let saved_at = fs::metadata(&path)
        .and_then(|metadata| metadata.modified())
        .ok()
        .and_then(|modified| modified.duration_since(UNIX_EPOCH).ok());
    if let Some(saved_at) = saved_at {
        let millis = saved_at.as_millis() as u64;
        readings.push(Reading::current_only("claude.limits_as_of", millis));
    }
    readings
}

/// A window whose reset time has passed is empty, whatever the file last said.
fn window_readings(prefix: &str, window: Option<LimitWindow>, now: i64) -> Vec<Reading> {
    let Some(window) = window else { return vec![] };
    let usage_key = format!("{prefix}.usage");
    let resets_key = format!("{prefix}.resets_at");

    if window.resets_at <= now {
        return vec![
            Reading::new(&usage_key, 0.0),
            Reading::current_only(&resets_key, Json::Null),
        ];
    }
    vec![
        Reading::new(&usage_key, window.used_percentage),
        Reading::current_only(&resets_key, window.resets_at * 1000),
    ]
}

#[derive(Deserialize)]
struct Entry {
    #[serde(rename = "requestId")]
    request_id: Option<String>,
    timestamp: Option<String>,
    message: Option<Message>,
}

#[derive(Deserialize)]
struct Message {
    model: Option<String>,
    usage: Option<Usage>,
}

/// Cache reads are left out: they are far larger than everything else and would
/// drown the numbers that show how much work was done.
#[derive(Default, Deserialize)]
#[serde(default)]
struct Usage {
    input_tokens: u64,
    output_tokens: u64,
    cache_creation_input_tokens: u64,
}

impl Usage {
    fn total(&self) -> u64 {
        self.input_tokens + self.output_tokens + self.cache_creation_input_tokens
    }
}

#[derive(Default)]
struct Tally {
    five_hour: u64,
    seven_day: u64,
    models: HashMap<String, u64>,
}

impl Tally {
    fn models_summary(&self) -> Json {
        let mut models: Vec<_> = self.models.iter().collect();
        models.sort_by(|a, b| b.1.cmp(a.1));
        let rows: Vec<_> = models
            .into_iter()
            .take(MODELS_SHOWN)
            .map(|(model, tokens)| {
                json!({
                    "label": model.trim_start_matches("claude-"),
                    "value": format!("{} tokens", short_count(*tokens)),
                })
            })
            .collect();
        rows.into()
    }
}

fn short_count(count: u64) -> String {
    match count {
        n if n >= 1_000_000 => format!("{:.1}M", n as f64 / 1_000_000.0),
        n if n >= 1_000 => format!("{:.0}K", n as f64 / 1_000.0),
        n => n.to_string(),
    }
}

/// Adds up every transcript touched within the week. Claude Code repeats a
/// response's usage on several lines, so each request counts once.
fn tally_tokens(projects: &Path, now: i64) -> Tally {
    let oldest = now - SEVEN_DAYS;
    let mut tally = Tally::default();
    let mut seen = HashSet::new();

    for path in transcripts(projects, oldest) {
        let Ok(bytes) = fs::read(&path) else { continue };
        for line in bytes.split(|&byte| byte == b'\n') {
            let Ok(line) = std::str::from_utf8(line) else {
                continue;
            };
            if !line.contains("\"usage\"") {
                continue;
            }
            let Ok(entry) = serde_json::from_str::<Entry>(line) else {
                continue;
            };
            add_entry(&mut tally, &mut seen, entry, now);
        }
    }
    tally
}

fn add_entry(tally: &mut Tally, seen: &mut HashSet<String>, entry: Entry, now: i64) {
    let (Some(request), Some(timestamp), Some(message)) =
        (entry.request_id, entry.timestamp, entry.message)
    else {
        return;
    };
    let Some(usage) = message.usage else { return };
    let Ok(at) = OffsetDateTime::parse(&timestamp, &Rfc3339) else {
        return;
    };

    let age = now - at.unix_timestamp();
    if age > SEVEN_DAYS || !seen.insert(request) {
        return;
    }

    let tokens = usage.total();
    tally.seven_day += tokens;
    if age <= FIVE_HOURS {
        tally.five_hour += tokens;
    }
    if let Some(model) = message.model.filter(|model| model != "<synthetic>") {
        *tally.models.entry(model).or_default() += tokens;
    }
}

/// Transcript files at any depth modified since `oldest`.
fn transcripts(root: &Path, oldest: i64) -> Vec<PathBuf> {
    let mut found = vec![];
    let mut folders = vec![root.to_path_buf()];

    while let Some(folder) = folders.pop() {
        let Ok(entries) = fs::read_dir(&folder) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let Ok(metadata) = entry.metadata() else {
                continue;
            };
            if metadata.is_dir() {
                folders.push(path);
            } else if path
                .extension()
                .is_some_and(|extension| extension == "jsonl")
            {
                let modified = metadata
                    .modified()
                    .ok()
                    .and_then(|modified| modified.duration_since(UNIX_EPOCH).ok())
                    .map_or(0, |since| since.as_secs() as i64);
                if modified >= oldest {
                    found.push(path);
                }
            }
        }
    }
    found
}
