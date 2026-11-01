//! Which tabs the user wants, and in what order. A hidden tab isn't only out of
//! sight: its source stops, so it costs nothing until it is shown again.

use serde::{Deserialize, Serialize};
use serde_json::to_value;
use tauri::{AppHandle, Manager};

use crate::{api::AppState, tray};

const TABS_SETTING: &str = "tabs";

#[derive(Clone, Deserialize, Serialize)]
pub struct TabChoice {
    pub source_id: String,
    #[serde(default)]
    pub name: String,
    pub shown: bool,
}

/// Every source in the user's order. One they have never seen comes last, shown.
pub fn choices(state: &AppState) -> Vec<TabChoice> {
    let saved: Vec<TabChoice> = state
        .store
        .setting(TABS_SETTING)
        .ok()
        .flatten()
        .and_then(|value| serde_json::from_value(value).ok())
        .unwrap_or_default();

    let mut choices: Vec<TabChoice> = saved
        .into_iter()
        .filter_map(|choice| {
            let source = state.source(&choice.source_id)?;
            Some(TabChoice {
                name: source.name().to_owned(),
                ..choice
            })
        })
        .collect();
    for source in &state.sources {
        if !choices.iter().any(|choice| choice.source_id == source.id()) {
            choices.push(TabChoice {
                source_id: source.id().to_owned(),
                name: source.name().to_owned(),
                shown: true,
            });
        }
    }
    choices
}

/// Starts or stops each source to match the user's choices.
pub fn apply(state: &AppState) {
    for choice in choices(state) {
        state.scheduler.set_running(&choice.source_id, choice.shown);
    }
}

/// Saves the user's choices and acts on them. At least one tab always stays.
pub fn choose(app: &AppHandle, choices: Vec<TabChoice>) -> Result<(), String> {
    if !choices.iter().any(|choice| choice.shown) {
        return Err("Keep at least one tab".to_owned());
    }
    let state = app.state::<AppState>();
    let value = to_value(&choices).map_err(|e| e.to_string())?;
    state
        .store
        .set_setting(TABS_SETTING, &value)
        .map_err(|e| e.to_string())?;
    apply(&state);
    tray::keep_stat_on_shown_tabs(app);
    Ok(())
}
