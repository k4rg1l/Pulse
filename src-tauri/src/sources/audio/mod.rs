//! Sound on this computer. Windows announces every change, so this source is told
//! rather than polling, and what it knows stays in memory: nothing about sound is
//! ever written to disk.

mod capture;
mod default_device;
mod devices;
mod media;
mod mixer;
mod worker;

use std::{
    collections::HashMap,
    sync::{Arc, Mutex, MutexGuard},
    time::Duration,
};

use async_trait::async_trait;
use serde::Deserialize;
use serde_json::Value as Json;
use tokio::sync::Notify;

use super::{LiveSink, Metric, Reading, Source, Widget};
use worker::Worker;

/// A safety net only, as Windows reports changes the moment they happen.
const INTERVAL: Duration = Duration::from_secs(60);

/// Whether something plays sound or records it.
#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Flow {
    Output,
    Input,
}

/// A playback button.
#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Transport {
    Previous,
    Toggle,
    Next,
}

/// Something the user does on the Audio tab.
#[derive(Debug, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum Action {
    SetDevice { id: String },
    SetVolume { flow: Flow, value: f32 },
    SetMuted { flow: Flow, muted: bool },
    SetAppVolume { app: String, value: f32 },
    SetAppMuted { app: String, muted: bool },
    Transport { command: Transport },
}

#[derive(Default)]
pub struct AudioMixer {
    worker: Mutex<Option<Worker>>,
}

impl AudioMixer {
    fn worker(&self) -> MutexGuard<'_, Option<Worker>> {
        self.worker
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn with_worker<T>(&self, use_it: impl FnOnce(&Worker) -> T) -> Result<T, String> {
        self.worker()
            .as_ref()
            .map(use_it)
            .ok_or_else(|| "Audio is not running".to_owned())
    }
}

#[async_trait]
impl Source for AudioMixer {
    fn id(&self) -> &'static str {
        "audio"
    }

    fn name(&self) -> &'static str {
        "Audio"
    }

    fn interval(&self) -> Duration {
        INTERVAL
    }

    fn widgets(&self) -> Vec<Widget> {
        Vec::new()
    }

    fn metrics(&self) -> Vec<Metric> {
        Vec::new()
    }

    fn watch(&self, changed: Arc<Notify>) {
        *self.worker() = Some(Worker::start(changed));
    }

    fn unwatch(&self) {
        if let Some(worker) = self.worker().take() {
            worker.stop();
        }
    }

    fn state(&self) -> Option<Json> {
        self.worker().as_ref()?.state().ok()
    }

    fn act(&self, action: Json) -> Result<(), String> {
        let action = serde_json::from_value(action).map_err(|e| e.to_string())?;
        self.with_worker(|worker| worker.perform(action))?
    }

    fn set_live(&self, sink: Option<LiveSink>) {
        let _ = self.with_worker(|worker| worker.set_live(sink));
    }

    async fn run(&self, _api_keys: HashMap<String, String>) -> Result<Vec<Reading>, String> {
        self.with_worker(|worker| {
            worker.refresh();
            worker.state().map(|_| Vec::new())
        })?
    }
}
