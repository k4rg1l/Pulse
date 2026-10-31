mod api;
mod scheduler;
mod secrets;
mod sources;
mod store;
mod tray;

use std::sync::Arc;

use tauri::{Manager, RunEvent, WindowEvent};

pub use sources::claude_status_line::{HOOK_FLAG as CLAUDE_HOOK_FLAG, run_hook as run_claude_hook};

use api::AppState;
use scheduler::{OnChange, Scheduler};
use store::Store;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        // Two servers would write to the same database, so a second launch
        // opens the window of the first and exits.
        .plugin(tauri_plugin_single_instance::init(|app, _, _| {
            tray::open_window(app, None)
        }))
        .plugin(tray::shortcut::plugin())
        .setup(|app| {
            secrets::init()?;

            let data_dir = app.path().app_data_dir()?;
            std::fs::create_dir_all(&data_dir)?;
            let store = Arc::new(Store::open(&data_dir.join("pulse.db"))?);

            let sources = sources::all();
            let scheduler = Arc::new(Scheduler::new(&sources));
            app.manage(AppState {
                store: store.clone(),
                sources: sources.clone(),
                scheduler: scheduler.clone(),
            });

            let handle = app.handle().clone();
            tray::setup(&handle)?;
            tray::prepare_client(&handle);
            tray::shortcut::register_saved(&handle);

            let on_change: OnChange = Arc::new(move |source_id| {
                tray::source_changed(&handle, source_id);
                api::notify_source_changed(&handle, source_id);
            });
            tauri::async_runtime::spawn(scheduler.run(sources, store, on_change));
            Ok(())
        })
        .on_window_event(|window, event| match event {
            WindowEvent::CloseRequested { api, .. } => {
                api.prevent_close();
                tray::close_window(window.app_handle());
            }
            // Clicking anywhere else dismisses the popover.
            WindowEvent::Focused(false) => tray::close_window(window.app_handle()),
            _ => {}
        })
        .invoke_handler(tauri::generate_handler![
            api::list_tabs,
            api::get_tab,
            api::get_history,
            api::client_ready,
            api::refresh_source,
            api::connect_source,
            api::disconnect_source,
            api::get_settings,
            api::update_setting,
            api::get_api_key_status,
            api::set_api_key,
            api::get_shortcut,
            api::set_shortcut,
            api::pause_shortcut,
        ])
        .build(tauri::generate_context!())
        .expect("error while building Pulse")
        .run(|_, event| {
            // With no window open Pulse keeps running. Only Quit, which sets an
            // exit code, ends it.
            if let RunEvent::ExitRequested {
                code: None, api, ..
            } = event
            {
                api.prevent_exit();
            }
        });
}
