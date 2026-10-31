//! The system-wide shortcut that opens and closes the popover.
//!
//! Shortcuts are saved in the plugin's own text form, such as `Ctrl+Alt+KeyP`.
//! The setting holds that text, or null once the user has turned the shortcut off.
//! With no setting at all, the default applies.

use serde::Serialize;
use serde_json::{Value as Json, json};
use tauri::{AppHandle, Manager, Wry, plugin::TauriPlugin};
use tauri_plugin_global_shortcut::{Builder, GlobalShortcutExt, Shortcut, ShortcutState};

use crate::api::AppState;

const SETTING: &str = "open_shortcut";

/// P for Pulse. Ctrl+Alt+Space looked like the natural choice, but other desktop
/// apps already claim it.
const DEFAULT_SHORTCUT: &str = "Ctrl+Alt+KeyP";

const IN_USE: &str = "Another app is using that one";

#[derive(Serialize)]
pub struct ShortcutStatus {
    shortcut: Option<String>,
    /// False when another app holds the saved shortcut, so pressing it does nothing.
    registered: bool,
}

/// Only the press acts, or a single tap would open and then close the popover.
pub fn plugin() -> TauriPlugin<Wry> {
    Builder::new()
        .with_handler(|app, _, event| {
            if event.state() == ShortcutState::Pressed {
                super::toggle_window(app, None);
            }
        })
        .build()
}

fn saved(app: &AppHandle) -> Option<String> {
    match app.state::<AppState>().store.setting(SETTING) {
        Ok(Some(Json::String(shortcut))) => Some(shortcut),
        Ok(Some(_)) => None,
        _ => Some(DEFAULT_SHORTCUT.to_owned()),
    }
}

/// Listens for the saved shortcut. If another app holds it, Settings says so.
pub fn register_saved(app: &AppHandle) {
    if let Some(shortcut) = saved(app) {
        let _ = app.global_shortcut().register(shortcut.as_str());
    }
}

pub fn status(app: &AppHandle) -> ShortcutStatus {
    let shortcut = saved(app);
    let registered = shortcut.as_ref().is_some_and(|shortcut| {
        shortcut
            .parse::<Shortcut>()
            .is_ok_and(|parsed| app.global_shortcut().is_registered(parsed))
    });
    ShortcutStatus {
        shortcut,
        registered,
    }
}

/// Stops listening while the user records a new shortcut, so that pressing the
/// current one is recorded instead of closing the popover.
pub fn pause(app: &AppHandle, paused: bool) -> Result<(), String> {
    app.global_shortcut()
        .unregister_all()
        .map_err(|e| e.to_string())?;
    if !paused {
        register_saved(app);
    }
    Ok(())
}

/// Saves a recorded shortcut, or `None` to turn it off. This runs while paused. On
/// a failure it stays paused, because the user is still recording.
pub fn set(app: &AppHandle, shortcut: Option<String>) -> Result<ShortcutStatus, String> {
    let manager = app.global_shortcut();
    manager.unregister_all().map_err(|e| e.to_string())?;

    if let Some(shortcut) = &shortcut {
        let parsed: Shortcut = shortcut
            .parse()
            .map_err(|_| "Pulse can't use that key".to_owned())?;
        manager.register(parsed).map_err(|_| IN_USE.to_owned())?;
    }

    app.state::<AppState>()
        .store
        .set_setting(SETTING, &json!(shortcut))
        .map_err(|e| e.to_string())?;
    Ok(status(app))
}
