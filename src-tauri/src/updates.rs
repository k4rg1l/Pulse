//! Keeping Pulse current. It looks for a new release now and then, tells the
//! client, and installs one only when the user asks.

use std::{sync::Mutex, time::Duration};

use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_updater::{Update, UpdaterExt};

const CHECK_EVERY: Duration = Duration::from_secs(6 * 60 * 60);

const UPDATE_AVAILABLE: &str = "update_available";

/// The newest release found, until it is installed.
#[derive(Default)]
pub struct AvailableUpdate(Mutex<Option<Update>>);

impl AvailableUpdate {
    fn current(&self) -> Option<Update> {
        self.0
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone()
    }

    fn replace(&self, update: Update) {
        *self
            .0
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(update);
    }
}

/// Looks for a new release for as long as Pulse runs. A development build never
/// does, as an update would replace the installed Pulse rather than itself.
pub fn watch(app: &AppHandle) {
    if cfg!(debug_assertions) {
        return;
    }
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        loop {
            // Being offline is normal, and the next check tries again.
            let _ = check(&app).await;
            tokio::time::sleep(CHECK_EVERY).await;
        }
    });
}

async fn check(app: &AppHandle) -> tauri_plugin_updater::Result<()> {
    if let Some(update) = app.updater()?.check().await? {
        let version = update.version.clone();
        app.state::<AvailableUpdate>().replace(update);
        let _ = app.emit(UPDATE_AVAILABLE, version);
    }
    Ok(())
}

pub fn available_version(app: &AppHandle) -> Option<String> {
    let update = app.state::<AvailableUpdate>().current()?;
    Some(update.version)
}

/// Downloads the update, rejects it unless its signature matches, and installs it.
/// Windows requires Pulse to exit for that, and the installer starts it again.
pub async fn install(app: &AppHandle) -> Result<(), String> {
    let update = app
        .state::<AvailableUpdate>()
        .current()
        .ok_or("There is no update to install")?;
    update
        .download_and_install(|_, _| {}, || {})
        .await
        .map_err(|e| e.to_string())
}
