//! What happens as Pulse starts: its first launch, and starting at login.

use serde_json::json;
use tauri::{AppHandle, Manager, Wry, plugin::TauriPlugin};
use tauri_plugin_autostart::{MacosLauncher, ManagerExt};

use crate::{api::AppState, tray};

const FIRST_LAUNCH_DONE_SETTING: &str = "first_launch_done";

pub fn plugin() -> TauriPlugin<Wry> {
    tauri_plugin_autostart::init(MacosLauncher::LaunchAgent, None)
}

/// A tray app shows nothing when it starts, which on the first launch looks like
/// it failed, so that one time the popover opens by itself. It also turns on
/// starting at login, which Settings can turn off.
pub fn run(app: &AppHandle) {
    let store = &app.state::<AppState>().store;
    let first_launch = !store
        .setting(FIRST_LAUNCH_DONE_SETTING)
        .is_ok_and(|done| done.is_some());

    if !first_launch {
        tray::prepare_client(app);
        return;
    }

    let _ = store.set_setting(FIRST_LAUNCH_DONE_SETTING, &json!(true));
    // A development build must not point the user's login at itself.
    if !cfg!(debug_assertions) {
        let _ = app.autolaunch().enable();
    }
    tray::open_window(app, None);
}

pub fn starts_at_login(app: &AppHandle) -> bool {
    app.autolaunch().is_enabled().unwrap_or(false)
}

pub fn set_starts_at_login(app: &AppHandle, on: bool) -> Result<(), String> {
    let autolaunch = app.autolaunch();
    let changed = if on {
        autolaunch.enable()
    } else {
        autolaunch.disable()
    };
    changed.map_err(|e| format!("Could not change starting at login: {e}"))
}
