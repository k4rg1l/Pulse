//! The only connection between client and server.
//!
//! Requests are Tauri commands. The data event, `source_changed`, carries only
//! the source ID, and the client requests what it needs in response. The tray
//! sends `popover_opened` and `popover_closing` for the panel's motion.

use std::sync::Arc;

use serde::Serialize;
use serde_json::Value as Json;
use tauri::{AppHandle, Emitter, State};

use crate::{
    scheduler::Scheduler,
    secrets,
    sources::{Metric, Setup, Source, Sources, Widget},
    store::{Setting, SourceError, Store, Value},
    tray,
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
pub struct TabSummary {
    source_id: &'static str,
    name: &'static str,
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
pub fn list_tabs(state: State<'_, AppState>) -> Vec<TabSummary> {
    state
        .sources
        .iter()
        .map(|source| TabSummary {
            source_id: source.id(),
            name: source.name(),
        })
        .collect()
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
    Ok(())
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
