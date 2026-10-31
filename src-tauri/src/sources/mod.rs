//! Sources are the unit of extension: adding a new kind of information means
//! adding one implementation of [`Source`] to [`all`].
//!
//! A source only fetches. It never touches the store, the client, or another
//! source. The scheduler decides when it runs and what happens to the result.

use std::{collections::HashMap, sync::Arc, time::Duration};

use async_trait::async_trait;
use serde::Serialize;
use serde_json::Value as Json;

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
    /// Capsule bar. Keys: percent, used, total.
    Meter,
    /// One large number.
    Stat,
    /// One bar per entry. Key holds an array of percentages.
    Bars,
    /// Rows with a capsule bar. Key holds an array of `{name, detail, used, total}`.
    List,
}

#[derive(Clone, Debug, Serialize)]
pub struct Widget {
    pub id: &'static str,
    #[serde(rename = "type")]
    pub kind: WidgetType,
    pub title: &'static str,
    pub keys: &'static [&'static str],
}

/// One value produced by a run.
#[derive(Debug)]
pub struct Reading {
    pub key: String,
    pub value: Json,
}

impl Reading {
    pub fn new(key: &str, value: impl Into<Json>) -> Self {
        Self { key: key.to_owned(), value: value.into() }
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

    /// One run: new data, or a message describing why there is none.
    async fn run(&self, api_keys: HashMap<String, String>) -> Result<Vec<Reading>, String>;
}

pub type Sources = Vec<Arc<dyn Source>>;

pub fn all() -> Sources {
    vec![Arc::new(system::SystemResources::default())]
}
