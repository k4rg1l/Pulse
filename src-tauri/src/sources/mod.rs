//! Sources are the unit of extension: adding a new kind of information means
//! adding one implementation of [`Source`] to [`all`].
//!
//! A source only fetches. It never touches the store, the client, or another
//! source. The scheduler decides when it runs and what happens to the result.

use std::{collections::HashMap, sync::Arc, time::Duration};

use async_trait::async_trait;
use serde::Serialize;
use serde_json::Value as Json;

pub mod claude;
pub mod claude_status_line;
pub mod system;

/// How long a run may take before the scheduler stops it and counts a failure.
const DEFAULT_TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Unit {
    Percent,
    Bytes,
    BytesPerSecond,
    Seconds,
    Celsius,
    Count,
    /// Milliseconds since the epoch.
    Timestamp,
}

/// A numeric key a source produces, with what the client and tray need to
/// display it. Any metric can be chosen as the tray stat.
#[derive(Clone, Debug, Serialize)]
pub struct Metric {
    pub key: &'static str,
    pub label: &'static str,
    pub unit: Unit,
}

#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum WidgetType {
    /// Line over time, read from history. One or more numeric keys.
    Chart,
    /// Two keys over time on one scale, the first above a centre line and the
    /// second below it, so they can never cover each other.
    MirrorChart,
    /// Capsule bar. Keys: percent, then used and total, or a reset timestamp.
    Meter,
    /// One large number.
    Stat,
    /// One bar per entry. Key holds an array of percentages.
    Bars,
    /// Rows with a capsule bar. Key holds an array of `{name, detail, used, total}`.
    List,
    /// Labelled text rows. Key holds an array of `{label, value}`.
    Details,
    /// One quiet line that closes the tab. Key holds text.
    Colophon,
}

#[derive(Clone, Debug, Serialize)]
pub struct Widget {
    pub id: &'static str,
    #[serde(rename = "type")]
    pub kind: WidgetType,
    pub title: &'static str,
    pub keys: &'static [&'static str],
    /// Key of a short text printed on the widget, naming what it measures.
    pub note: Option<&'static str>,
}

#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SetupState {
    /// What the source reads is not on this computer.
    Unavailable,
    /// The user can connect it.
    Disconnected,
    /// Connected, with no data yet.
    Waiting,
    Connected,
}

/// A one-time step a source needs from the user, shown on its tab and in Settings.
#[derive(Clone, Debug, Serialize)]
pub struct Setup {
    pub state: SetupState,
    pub title: &'static str,
    pub detail: &'static str,
    pub action: &'static str,
}

/// One value produced by a run.
#[derive(Debug)]
pub struct Reading {
    pub key: String,
    pub value: Json,
    pub keep_history: bool,
}

impl Reading {
    pub fn new(key: &str, value: impl Into<Json>) -> Self {
        Self {
            key: key.to_owned(),
            value: value.into(),
            keep_history: true,
        }
    }

    /// For constants and text, which only need their latest value.
    pub fn current_only(key: &str, value: impl Into<Json>) -> Self {
        Self {
            keep_history: false,
            ..Self::new(key, value)
        }
    }
}

#[async_trait]
pub trait Source: Send + Sync + 'static {
    fn id(&self) -> &'static str;
    fn name(&self) -> &'static str;
    fn interval(&self) -> Duration;
    fn widgets(&self) -> Vec<Widget>;
    fn metrics(&self) -> Vec<Metric>;

    /// Names of the API keys this source needs. It receives only these.
    fn api_keys(&self) -> &'static [&'static str] {
        &[]
    }

    fn timeout(&self) -> Duration {
        DEFAULT_TIMEOUT
    }

    /// A one-time step this source needs, if any.
    fn setup(&self) -> Option<Setup> {
        None
    }

    fn connect(&self) -> Result<(), String> {
        Ok(())
    }

    fn disconnect(&self) -> Result<(), String> {
        Ok(())
    }

    /// One run: new data, or a message describing why there is none.
    async fn run(&self, api_keys: HashMap<String, String>) -> Result<Vec<Reading>, String>;
}

pub type Sources = Vec<Arc<dyn Source>>;

pub fn all() -> Sources {
    vec![
        Arc::new(system::SystemResources::default()),
        Arc::new(claude::ClaudeUsage),
    ]
}
