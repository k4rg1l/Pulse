//! The only connection between client and server.
//!
//! Requests are Tauri commands. The data event, `source_changed`, carries only
//! the source ID, and the client requests what it needs in response. The tray
//! sends `popover_opened` and `popover_closing` for the panel's motion.

use std::sync::Arc;

use serde::Serialize;
use serde_json::Value as Json;
use tauri::{AppHandle, Emitter, State, ipc::Channel};

use crate::{
    scheduler::{Scheduler, now_ms},
    secrets,
    sources::{LiveSink, Metric, Setup, Source, Sources, Widget},
    startup,
    store::{HISTORY_DAYS_SETTING, Setting, SourceError, Store, Value},
    tabs::{self, TabChoice},
    tray::{
        self,
        shortcut::{self, ShortcutStatus},
    },
    updates,
};

pub const SOURCE_CHANGED: &str = "source_changed";

pub struct AppState {
    pub store: Arc<Store>,
    pub sources: Sources,
    pub scheduler: Arc<Scheduler>,
}

impl AppState {
    pub fn source(&self, source_id: &str) -> Option<&Arc<dyn Source>> {
        self.sources.iter().find(|source| source.id() == source_id)
    }

    fn require_source(&self, source_id: &str) -> Result<&Arc<dyn Source>, String> {
        self.source(source_id)
            .ok_or_else(|| format!("Unknown source: {source_id}"))
    }
}

/// Tells the client a source has changed. Without a client this goes nowhere.
pub fn notify_source_changed(app: &AppHandle, source_id: &str) {
    let _ = app.emit(SOURCE_CHANGED, source_id);
}

#[derive(Serialize)]
pub struct Tab {
    source_id: &'static str,
    name: &'static str,
    widgets: Vec<Widget>,
    metrics: Vec<Metric>,
    values: Vec<Value>,
    error: Option<SourceError>,
    setup: Option<Setup>,
    state: Option<Json>,
}

#[derive(Serialize)]
pub struct ApiKeyStatus {
    name: &'static str,
    is_set: bool,
}

fn message(error: impl std::fmt::Display) -> String {
    error.to_string()
}

#[tauri::command(async, rename_all = "snake_case")]
pub fn list_tabs(state: State<'_, AppState>) -> Vec<TabChoice> {
    tabs::choices(&state)
        .into_iter()
        .filter(|choice| choice.shown)
        .collect()
}

#[tauri::command(async, rename_all = "snake_case")]
pub fn get_tab_choices(state: State<'_, AppState>) -> Vec<TabChoice> {
    tabs::choices(&state)
}

#[tauri::command(async, rename_all = "snake_case")]
pub fn set_tab_choices(choices: Vec<TabChoice>, app: AppHandle) -> Result<(), String> {
    tabs::choose(&app, choices)
}

#[tauri::command(async, rename_all = "snake_case")]
pub fn get_tab(source_id: String, state: State<'_, AppState>) -> Result<Tab, String> {
    let source = state.require_source(&source_id)?;
    Ok(Tab {
        source_id: source.id(),
        name: source.name(),
        widgets: source.widgets(),
        metrics: source.metrics(),
        values: state.store.current(&source_id).map_err(message)?,
        error: state.store.active_error(&source_id).map_err(message)?,
        setup: source.setup(),
        state: source.state(),
    })
}

#[tauri::command(async, rename_all = "snake_case")]
pub fn get_history(
    source_id: String,
    key: String,
    from: i64,
    to: i64,
    max_points: Option<u32>,
    state: State<'_, AppState>,
) -> Result<Vec<Value>, String> {
    state
        .store
        .history(&source_id, &key, from, to, max_points)
        .map_err(message)
}

#[tauri::command(async, rename_all = "snake_case")]
pub fn client_ready(app: AppHandle) -> tray::ClientState {
    tray::client_ready(&app)
}

#[tauri::command(async, rename_all = "snake_case")]
pub fn refresh_source(source_id: String, state: State<'_, AppState>) -> Result<(), String> {
    state.require_source(&source_id)?;
    state.scheduler.refresh(&source_id);
    Ok(())
}

#[tauri::command(async, rename_all = "snake_case")]
pub fn act(source_id: String, action: Json, state: State<'_, AppState>) -> Result<(), String> {
    state.require_source(&source_id)?.act(action)
}

#[tauri::command(async, rename_all = "snake_case")]
pub fn start_live(
    source_id: String,
    channel: Channel<Json>,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let sink: LiveSink = Box::new(move |moment| channel.send(moment).is_ok());
    state.require_source(&source_id)?.set_live(Some(sink));
    Ok(())
}

#[tauri::command(async, rename_all = "snake_case")]
pub fn stop_live(source_id: String, state: State<'_, AppState>) -> Result<(), String> {
    state.require_source(&source_id)?.set_live(None);
    Ok(())
}

#[tauri::command(async, rename_all = "snake_case")]
pub fn connect_source(source_id: String, state: State<'_, AppState>) -> Result<(), String> {
    let source = state.require_source(&source_id)?;
    source.connect()?;
    state.scheduler.refresh(source.id());
    Ok(())
}

#[tauri::command(async, rename_all = "snake_case")]
pub fn disconnect_source(source_id: String, state: State<'_, AppState>) -> Result<(), String> {
    let source = state.require_source(&source_id)?;
    source.disconnect()?;
    state.scheduler.refresh(source.id());
    Ok(())
}

#[tauri::command(async, rename_all = "snake_case")]
pub fn get_settings(state: State<'_, AppState>) -> Result<Vec<Setting>, String> {
    state.store.settings().map_err(message)
}

#[tauri::command(async, rename_all = "snake_case")]
pub fn update_setting(
    key: String,
    value: Json,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<(), String> {
    state.store.set_setting(&key, &value).map_err(message)?;
    if key == tray::TRAY_STAT_SETTING {
        tray::refresh_stat(&app);
    }
    if key == HISTORY_DAYS_SETTING {
        state.store.prune_history(now_ms()).map_err(message)?;
    }
    Ok(())
}

#[tauri::command(async, rename_all = "snake_case")]
pub fn get_start_at_login(app: AppHandle) -> bool {
    startup::starts_at_login(&app)
}

#[tauri::command(async, rename_all = "snake_case")]
pub fn set_start_at_login(on: bool, app: AppHandle) -> Result<(), String> {
    startup::set_starts_at_login(&app, on)
}

#[tauri::command(async, rename_all = "snake_case")]
pub fn get_shortcut(app: AppHandle) -> ShortcutStatus {
    shortcut::status(&app)
}

#[tauri::command(async, rename_all = "snake_case")]
pub fn set_shortcut(shortcut: Option<String>, app: AppHandle) -> Result<ShortcutStatus, String> {
    shortcut::set(&app, shortcut)
}

#[tauri::command(async, rename_all = "snake_case")]
pub fn pause_shortcut(paused: bool, app: AppHandle) -> Result<(), String> {
    shortcut::pause(&app, paused)
}

#[tauri::command(async, rename_all = "snake_case")]
pub fn get_update(app: AppHandle) -> Option<String> {
    updates::available_version(&app)
}

#[tauri::command(rename_all = "snake_case")]
pub async fn check_for_update(app: AppHandle) -> Result<Option<String>, String> {
    updates::check(&app).await.map_err(message)
}

#[tauri::command(rename_all = "snake_case")]
pub async fn install_update(app: AppHandle) -> Result<(), String> {
    updates::install(&app).await
}

#[tauri::command(async, rename_all = "snake_case")]
pub fn get_api_key_status(
    source_id: String,
    state: State<'_, AppState>,
) -> Result<Vec<ApiKeyStatus>, String> {
    let source = state.require_source(&source_id)?;
    source
        .api_keys()
        .iter()
        .map(|&name| {
            Ok(ApiKeyStatus {
                name,
                is_set: secrets::is_set(source.id(), name)?,
            })
        })
        .collect()
}

#[tauri::command(async, rename_all = "snake_case")]
pub fn set_api_key(
    source_id: String,
    name: String,
    value: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let source = state.require_source(&source_id)?;
    if !source.api_keys().contains(&name.as_str()) {
        return Err(format!(
            "{} does not use a key called {name}",
            source.name()
        ));
    }
    secrets::set(source.id(), &name, &value)?;
    state.scheduler.refresh(source.id());
    Ok(())
}
