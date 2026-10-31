//! Codex usage, read through Codex's own app server.
//!
//! Unlike Claude there is nothing to connect: if Codex is installed and signed
//! in with ChatGPT, this works. See `codex_app_server` for how it talks to Codex.

use std::{
    collections::HashMap,
    sync::{Mutex, MutexGuard},
    time::Duration,
};

use async_trait::async_trait;
use serde::Deserialize;
use serde_json::{Value as Json, json};
use time::{Date, OffsetDateTime};

use crate::scheduler::now_ms;

use super::{
    Metric, Reading, Setup, SetupState, Source, Unit, Widget, WidgetType,
    codex_app_server::{AppServer, find_codex},
};

/// Each run is a real request to OpenAI, so this is slower than the local sources.
const INTERVAL: Duration = Duration::from_secs(120);

const DAYS_IN_WEEK: i64 = 7;
const MINUTES_IN_HOUR: u64 = 60;
const MINUTES_IN_DAY: u64 = 24 * MINUTES_IN_HOUR;
const MINUTES_IN_WEEK: u64 = DAYS_IN_WEEK as u64 * MINUTES_IN_DAY;

/// Everything this source reports, so that signing out leaves no old figures behind.
const KEYS: &[&str] = &[
    "codex.primary.usage",
    "codex.primary.resets_at",
    "codex.primary.window",
    "codex.primary.name",
    "codex.secondary.usage",
    "codex.secondary.resets_at",
    "codex.secondary.window",
    "codex.secondary.name",
    "codex.credits",
    "codex.credits.resets",
    "codex.tokens.today",
    "codex.tokens.week",
    "codex.tokens.trend",
    "codex.streak.current",
    "codex.streak.longest",
    "codex.daily",
    "codex.records",
    "codex.plan",
];

#[derive(Clone, Copy, Default)]
enum Login {
    /// Before the first run. Nothing is shown, so a working tab never flashes a
    /// setup card as Pulse starts.
    #[default]
    Unknown,
    NotInstalled,
    SignedOut,
    /// Limits and tokens belong to ChatGPT plans, so an API key has neither.
    ApiKey,
    ChatGpt,
}

#[derive(Default)]
pub struct CodexUsage {
    login: Mutex<Login>,
}

impl CodexUsage {
    /// A panic mid-write leaves only an older state, which the next run replaces.
    fn login(&self) -> MutexGuard<'_, Login> {
        self.login
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

#[async_trait]
impl Source for CodexUsage {
    fn id(&self) -> &'static str {
        "codex"
    }

    fn name(&self) -> &'static str {
        "Codex"
    }

    fn interval(&self) -> Duration {
        INTERVAL
    }

    fn widgets(&self) -> Vec<Widget> {
        vec![
            Widget {
                id: "primary-limit",
                kind: WidgetType::Pace,
                title: "Limit",
                keys: &[
                    "codex.primary.usage",
                    "codex.primary.resets_at",
                    "codex.primary.window",
                ],
                note: Some("codex.primary.name"),
            },
            Widget {
                id: "secondary-limit",
                kind: WidgetType::Pace,
                title: "Limit",
                keys: &[
                    "codex.secondary.usage",
                    "codex.secondary.resets_at",
                    "codex.secondary.window",
                ],
                note: Some("codex.secondary.name"),
            },
            Widget {
                id: "tokens-week",
                kind: WidgetType::Stat,
                title: "Tokens, 7 days",
                keys: &["codex.tokens.week"],
                note: Some("codex.tokens.trend"),
            },
            Widget {
                id: "tokens-today",
                kind: WidgetType::Stat,
                title: "Today",
                keys: &["codex.tokens.today"],
                note: None,
            },
            Widget {
                id: "credits",
                kind: WidgetType::Stat,
                title: "Credits",
                keys: &["codex.credits"],
                note: Some("codex.credits.resets"),
            },
            Widget {
                id: "streak",
                kind: WidgetType::Streak,
                title: "Streak",
                keys: &["codex.streak.current", "codex.streak.longest"],
                note: None,
            },
            Widget {
                id: "daily",
                kind: WidgetType::Calendar,
                title: "Daily tokens",
                keys: &["codex.daily"],
                note: None,
            },
            Widget {
                id: "records",
                kind: WidgetType::Records,
                title: "Records",
                keys: &["codex.records"],
                note: None,
            },
            Widget {
                id: "plan",
                kind: WidgetType::Colophon,
                title: "Plan",
                keys: &["codex.plan"],
                note: None,
            },
        ]
    }

    fn metrics(&self) -> Vec<Metric> {
        vec![
            Metric {
                key: "codex.primary.usage",
                label: "Codex limit",
                unit: Unit::Percent,
            },
            Metric {
                key: "codex.secondary.usage",
                label: "Codex second limit",
                unit: Unit::Percent,
            },
            Metric {
                key: "codex.credits",
                label: "Codex credits",
                unit: Unit::Amount,
            },
            Metric {
                key: "codex.tokens.today",
                label: "Codex tokens today",
                unit: Unit::Count,
            },
            Metric {
                key: "codex.tokens.week",
                label: "Codex tokens, 7 days",
                unit: Unit::Count,
            },
        ]
    }

    /// There is no step Pulse can take here, so a working source shows nothing
    /// and never offers to disconnect.
    fn setup(&self) -> Option<Setup> {
        let (title, detail) = match *self.login() {
            Login::Unknown | Login::ChatGpt => return None,
            Login::NotInstalled => (
                "Codex isn't installed",
                "Install Codex to see your usage here.",
            ),
            Login::SignedOut => (
                "Sign in to Codex",
                "Pulse uses Codex's own sign-in and never sees your login.",
            ),
            Login::ApiKey => (
                "Codex is using an API key",
                "Limits and tokens come with a ChatGPT sign-in, so sign in to Codex with ChatGPT.",
            ),
        };
        Some(Setup {
            state: SetupState::Unavailable,
            title,
            detail,
            action: "",
        })
    }

    async fn run(&self, _api_keys: HashMap<String, String>) -> Result<Vec<Reading>, String> {
        let Some(codex) = find_codex().await else {
            *self.login() = Login::NotInstalled;
            return Ok(cleared());
        };

        let mut server = AppServer::start(&codex).await?;
        let reply: AccountReply = server
            .request("account/read", json!({ "refreshToken": false }))
            .await?;

        let login = reply.login();
        *self.login() = login;
        if !matches!(login, Login::ChatGpt) {
            return Ok(cleared());
        }

        let limits = server
            .request("account/rateLimits/read", Json::Null)
            .await?;
        let usage = server.request("account/usage/read", Json::Null).await?;
        Ok(readings(limits, usage, now_ms() / 1000))
    }
}

/// A blank for every key, which is how a tab loses its figures.
fn cleared() -> Vec<Reading> {
    KEYS.iter()
        .map(|key| Reading::current_only(key, Json::Null))
        .collect()
}

#[derive(Deserialize)]
struct AccountReply {
    account: Option<Account>,
}

impl AccountReply {
    fn login(&self) -> Login {
        match self.account.as_ref().map(|account| account.kind.as_str()) {
            None => Login::SignedOut,
            Some("chatgpt") => Login::ChatGpt,
            Some(_) => Login::ApiKey,
        }
    }
}

#[derive(Deserialize)]
struct Account {
    #[serde(rename = "type")]
    kind: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct LimitsReply {
    rate_limits: Limits,
    rate_limit_reset_credits: Option<ResetCredits>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Limits {
    primary: Option<LimitWindow>,
    secondary: Option<LimitWindow>,
    credits: Option<Credits>,
    plan_type: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct LimitWindow {
    used_percent: f64,
    window_duration_mins: Option<u64>,
    resets_at: Option<i64>,
}

#[derive(Deserialize)]
struct Credits {
    balance: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ResetCredits {
    available_count: Option<u32>,
    #[serde(default)]
    credits: Vec<ResetCredit>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ResetCredit {
    expires_at: Option<i64>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct UsageReply {
    summary: Summary,
    daily_usage_buckets: Option<Vec<DailyTokens>>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Summary {
    lifetime_tokens: Option<u64>,
    peak_daily_tokens: Option<u64>,
    longest_running_turn_sec: Option<u64>,
    current_streak_days: Option<u64>,
    longest_streak_days: Option<u64>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct DailyTokens {
    start_date: String,
    tokens: u64,
}

/// Everything a successful run reports.
fn readings(limits: LimitsReply, usage: UsageReply, now: i64) -> Vec<Reading> {
    let LimitsReply {
        rate_limits,
        rate_limit_reset_credits,
    } = limits;
    let days = usage.daily_usage_buckets.unwrap_or_default();
    let today = OffsetDateTime::now_local()
        .unwrap_or_else(|_| OffsetDateTime::now_utc())
        .date();

    let week = tokens_in_week_ending(&days, today);
    let week_before = tokens_in_week_ending(
        &days,
        today.saturating_sub(time::Duration::days(DAYS_IN_WEEK)),
    );

    let mut readings = window_readings("codex.primary", rate_limits.primary, now);
    readings.extend(window_readings(
        "codex.secondary",
        rate_limits.secondary,
        now,
    ));
    readings.extend([
        Reading::current_only(
            "codex.credits.resets",
            resets_summary(rate_limit_reset_credits),
        ),
        Reading::current_only("codex.tokens.today", tokens_on(&days, today)),
        Reading::current_only("codex.tokens.week", week),
        Reading::current_only("codex.tokens.trend", week_trend(week, week_before)),
        Reading::current_only("codex.streak.current", usage.summary.current_streak_days),
        Reading::current_only("codex.streak.longest", usage.summary.longest_streak_days),
        Reading::current_only("codex.records", records(&usage.summary, &days)),
        Reading::current_only("codex.daily", daily(&days)),
        Reading::current_only(
            "codex.plan",
            rate_limits.plan_type.as_deref().map(plan_name),
        ),
    ]);

    let balance = rate_limits
        .credits
        .and_then(|credits| credits.balance?.parse::<f64>().ok());
    if let Some(balance) = balance {
        readings.push(Reading::new("codex.credits", balance));
    }
    readings
}

/// Times are in milliseconds, like every timestamp Pulse stores. A window that has
/// not started has no reset time yet, so it is drawn as starting now.
fn window_readings(prefix: &str, window: Option<LimitWindow>, now: i64) -> Vec<Reading> {
    let key = |name: &str| format!("{prefix}.{name}");
    let Some(LimitWindow {
        used_percent,
        window_duration_mins: Some(minutes),
        resets_at,
    }) = window
    else {
        return ["usage", "resets_at", "window", "name"]
            .map(|name| Reading::current_only(&key(name), Json::Null))
            .into();
    };

    let window_seconds = minutes as i64 * 60;
    let resets_at = resets_at.unwrap_or(now + window_seconds);
    vec![
        Reading::new(&key("usage"), used_percent),
        Reading::current_only(&key("resets_at"), resets_at * 1000),
        Reading::current_only(&key("window"), window_seconds * 1000),
        Reading::current_only(&key("name"), window_name(minutes)),
    ]
}

/// What a limit is called, from its length alone, since plans differ.
fn window_name(minutes: u64) -> String {
    match minutes {
        MINUTES_IN_WEEK => "Weekly limit".to_owned(),
        MINUTES_IN_DAY => "Daily limit".to_owned(),
        m if m % MINUTES_IN_DAY == 0 => format!("{}-day limit", m / MINUTES_IN_DAY),
        m if m % MINUTES_IN_HOUR == 0 => format!("{}-hour limit", m / MINUTES_IN_HOUR),
        m => format!("{m}-minute limit"),
    }
}

/// How many free limit resets are ready, and when the first of them lapses.
fn resets_summary(resets: Option<ResetCredits>) -> Option<String> {
    let resets = resets?;
    let count = resets.available_count?;
    let label = if count == 1 {
        "free reset"
    } else {
        "free resets"
    };
    let lapses = resets
        .credits
        .iter()
        .filter_map(|credit| credit.expires_at)
        .min()
        .and_then(|seconds| OffsetDateTime::from_unix_timestamp(seconds).ok());

    Some(match (count, lapses) {
        (0, _) => "No free resets".to_owned(),
        (_, None) => format!("{count} {label}"),
        (_, Some(date)) => {
            let month = date.month().to_string();
            format!("{count} {label} until {} {}", &month[..3], date.day())
        }
    })
}

/// Tokens used on one date. Days with no use are simply absent.
fn tokens_on(days: &[DailyTokens], date: Date) -> u64 {
    let date = date.to_string();
    days.iter()
        .find(|day| day.start_date == date)
        .map_or(0, |day| day.tokens)
}

/// Dates are ISO text, which sorts the same way the days fall.
fn tokens_in_week_ending(days: &[DailyTokens], end: Date) -> u64 {
    let first = end
        .saturating_sub(time::Duration::days(DAYS_IN_WEEK - 1))
        .to_string();
    let last = end.to_string();
    days.iter()
        .filter(|day| day.start_date >= first && day.start_date <= last)
        .map(|day| day.tokens)
        .sum()
}

/// This week against the one before, in words.
fn week_trend(week: u64, week_before: u64) -> String {
    if week_before == 0 {
        return "Nothing the week before".to_owned();
    }
    let change = (week as f64 / week_before as f64 - 1.0) * 100.0;
    match change.round() as i64 {
        0 => "Same as the week before".to_owned(),
        c if c > 0 => format!("{c}% more than the week before"),
        c => format!("{}% less than the week before", -c),
    }
}

/// The all-time figures, each with the unit that formats it.
fn records(summary: &Summary, days: &[DailyTokens]) -> Json {
    let active_days = days.iter().filter(|day| day.tokens > 0).count() as u64;
    let average_day = summary
        .lifetime_tokens
        .map(|tokens| tokens / active_days.max(1));
    json!([
        { "label": "Lifetime tokens", "value": summary.lifetime_tokens, "unit": Unit::Count },
        { "label": "Peak day", "value": summary.peak_daily_tokens, "unit": Unit::Count },
        { "label": "Longest turn", "value": summary.longest_running_turn_sec, "unit": Unit::Seconds },
        { "label": "Average active day", "value": average_day, "unit": Unit::Count },
    ])
}

/// The day-by-day series for the calendar.
fn daily(days: &[DailyTokens]) -> Json {
    days.iter()
        .map(|day| json!({ "date": day.start_date, "value": day.tokens }))
        .collect::<Vec<_>>()
        .into()
}

/// Turns a plan id such as `pro` into "ChatGPT Pro".
fn plan_name(plan: &str) -> String {
    let mut letters = plan.chars();
    let name: String = match letters.next() {
        Some(first) => first.to_uppercase().chain(letters).collect(),
        None => return String::new(),
    };
    format!("ChatGPT {}", name.replace('_', " "))
}
