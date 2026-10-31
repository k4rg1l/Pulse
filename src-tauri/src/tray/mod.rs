//! Keeps Pulse alive with no window open: owns the tray icon and menu, shows the
//! chosen stat, and opens and closes the popover anchored to the icon.

use std::{
    sync::{
        Mutex,
        atomic::{AtomicU64, Ordering},
    },
    time::{Duration, Instant},
};

use serde::{Deserialize, Serialize};
use tauri::{
    AppHandle, Emitter, Manager, PhysicalPosition, PhysicalSize, Rect, WebviewUrl, WebviewWindow,
    WebviewWindowBuilder,
    image::Image,
    menu::{Menu, MenuItem, PredefinedMenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
};

use crate::api::AppState;

pub mod icon;
mod popover;
pub mod shortcut;

pub use popover::Anchor;
use popover::{Bounds, Placement};

pub const TRAY_STAT_SETTING: &str = "tray_stat";

/// When on, the client is started with Pulse and never shut down, so the popover
/// opens instantly for about 250 MB more memory.
const KEEP_CLIENT_READY_SETTING: &str = "keep_client_ready";

const TRAY_ID: &str = "pulse";
const WINDOW_LABEL: &str = "main";

/// How long a closed window stays alive. Reopening within this is instant.
/// After it the client is destroyed and its memory released.
const SHUTDOWN_DELAY: Duration = Duration::from_secs(30);

/// How long the client gets to fade the panel out before the window is hidden.
/// A hidden window keeps its last frame, which would flash on the next open.
const FADE_OUT: Duration = Duration::from_millis(90);

/// Pressing the tray icon while the popover is open dismisses it through the
/// lost focus, just before the release arrives as a click. A click this soon
/// after a dismissal is that same press, and must not reopen it.
const REOPEN_GUARD: Duration = Duration::from_millis(350);

/// A new client shows the popover itself once it has rendered. If it never
/// reports in, the popover is shown anyway rather than appearing to do nothing.
const READY_TIMEOUT: Duration = Duration::from_secs(3);

const POPOVER_OPENED: &str = "popover_opened";
const POPOVER_CLOSING: &str = "popover_closing";

#[derive(Deserialize)]
struct TrayStat {
    source_id: String,
    key: String,
}

impl Default for TrayStat {
    fn default() -> Self {
        Self {
            source_id: "system".to_owned(),
            key: "cpu.usage".to_owned(),
        }
    }
}

/// What happens to a client once it has finished starting.
enum Pending {
    Show(Option<Placement>),
    StayHidden,
}

/// What a client needs to know when it has finished starting.
#[derive(Serialize)]
pub struct ClientState {
    anchor: Option<Anchor>,
    /// False for a client started ahead of time, which must wait to be opened.
    shown: bool,
}

#[derive(Default)]
struct TrayState {
    /// Raised by every open and close, so that a pending shutdown can tell it
    /// has been superseded.
    window_generation: AtomicU64,
    /// What the icon currently shows, to avoid redrawing an unchanged stat.
    shown: Mutex<Option<(String, Option<u8>)>>,
    closed_at: Mutex<Option<Instant>>,
    anchor: Mutex<Option<Anchor>>,
    /// Set while a new client is starting.
    pending: Mutex<Option<Pending>>,
}

pub fn setup(app: &AppHandle) -> tauri::Result<()> {
    app.manage(TrayState::default());

    let open = MenuItem::with_id(app, "open", "Open Pulse", true, None::<&str>)?;
    let separator = PredefinedMenuItem::separator(app)?;
    let quit = MenuItem::with_id(app, "quit", "Quit Pulse", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&open, &separator, &quit])?;

    TrayIconBuilder::with_id(TRAY_ID)
        .icon(tile("--", None))
        .tooltip("Pulse")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id().as_ref() {
            "open" => open_window(app, None),
            "quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                rect,
                ..
            } = event
            {
                toggle_window(tray.app_handle(), Some(rect));
            }
        })
        .build(app)?;

    Ok(())
}

fn tile(text: &str, level: Option<f32>) -> Image<'static> {
    Image::new_owned(icon::render(text, level), icon::SIZE, icon::SIZE)
}

/// Redraws the tray if the source that just changed provides the chosen stat.
pub fn source_changed(app: &AppHandle, source_id: &str) {
    if chosen_stat(app).source_id == source_id {
        refresh_stat(app);
    }
}

fn chosen_stat(app: &AppHandle) -> TrayStat {
    app.state::<AppState>()
        .store
        .setting(TRAY_STAT_SETTING)
        .ok()
        .flatten()
        .and_then(|value| serde_json::from_value(value).ok())
        .unwrap_or_default()
}

/// Shows the chosen stat on the icon and in its tooltip.
pub fn refresh_stat(app: &AppHandle) {
    let state = app.state::<AppState>();
    let stat = chosen_stat(app);

    let metric = state.source(&stat.source_id).and_then(|source| {
        source
            .metrics()
            .into_iter()
            .find(|metric| metric.key == stat.key)
    });
    let Some(metric) = metric else { return };
    let Ok(Some(value)) = state.store.current_number(&stat.source_id, &stat.key) else {
        return;
    };

    let text = icon::short_text(metric.unit, value);
    let level = icon::level(metric.unit, value);
    // Whole percents are as fine as the tile can show.
    let shown = (text.clone(), level.map(|level| (level * 100.0) as u8));

    let tray_state = app.state::<TrayState>();
    let mut last = tray_state
        .shown
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if last.as_ref() == Some(&shown) {
        return;
    }

    if let Some(tray) = app.tray_by_id(TRAY_ID) {
        let tooltip = format!(
            "Pulse: {} {}",
            metric.label,
            icon::long_text(metric.unit, value)
        );
        let _ = tray.set_icon(Some(tile(&text, level)));
        let _ = tray.set_tooltip(Some(tooltip));
        *last = Some(shown);
    }
}

fn window(app: &AppHandle) -> Option<WebviewWindow> {
    app.get_webview_window(WINDOW_LABEL)
}

fn keeps_client_ready(app: &AppHandle) -> bool {
    app.state::<AppState>()
        .store
        .setting(KEEP_CLIENT_READY_SETTING)
        .ok()
        .flatten()
        .is_some_and(|value| value == true)
}

/// Starts a hidden client along with Pulse, if the user keeps one ready.
pub fn prepare_client(app: &AppHandle) {
    if keeps_client_ready(app) {
        *app.state::<TrayState>()
            .pending
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(Pending::StayHidden);
        start_client(app);
    }
}

/// Opens the popover, or closes it if it is already open.
fn toggle_window(app: &AppHandle, icon: Option<Rect>) {
    if window(app).is_some_and(|window| window.is_visible().unwrap_or(false)) {
        close_window(app);
        return;
    }

    let state = app.state::<TrayState>();
    let closed_at = *state
        .closed_at
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if !closed_at.is_some_and(|at| at.elapsed() < REOPEN_GUARD) {
        open_window(app, icon);
    }
}

fn bounds_of(rect: Rect, scale: f64) -> Bounds {
    let position = rect.position.to_physical::<f64>(scale);
    let size = rect.size.to_physical::<f64>(scale);
    Bounds {
        x: position.x,
        y: position.y,
        width: size.width,
        height: size.height,
    }
}

/// Works out where the popover goes for this icon. Without a click to go on, it
/// asks the tray where the icon is, and failing that assumes the screen corner.
fn placement(app: &AppHandle, icon: Option<Rect>) -> Option<Placement> {
    let icon = icon.or_else(|| app.tray_by_id(TRAY_ID)?.rect().ok().flatten());
    let monitor = icon.and_then(|rect| {
        let position = rect.position.to_physical::<f64>(1.0);
        app.monitor_from_point(position.x, position.y)
            .ok()
            .flatten()
    });
    let monitor = monitor.or_else(|| app.primary_monitor().ok().flatten())?;

    let scale = monitor.scale_factor();
    let area = monitor.work_area();
    let work_area = Bounds {
        x: f64::from(area.position.x),
        y: f64::from(area.position.y),
        width: f64::from(area.size.width),
        height: f64::from(area.size.height),
    };
    let corner = Bounds {
        x: work_area.x + work_area.width - 40.0 * scale,
        y: work_area.y + work_area.height + scale,
        width: 24.0 * scale,
        height: 24.0 * scale,
    };

    Some(popover::place(
        icon.map_or(corner, |rect| bounds_of(rect, scale)),
        work_area,
        scale,
    ))
}

fn present(app: &AppHandle, window: &WebviewWindow, placement: Option<Placement>) {
    if let Some(Placement {
        window: bounds,
        anchor,
    }) = placement
    {
        let position = PhysicalPosition::new(bounds.x.round() as i32, bounds.y.round() as i32);
        let size = PhysicalSize::new(bounds.width.round() as u32, bounds.height.round() as u32);
        let _ = window.set_position(position);
        let _ = window.set_size(size);
        *app.state::<TrayState>()
            .anchor
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(anchor);
    }

    let _ = window.show();
    let _ = window.set_focus();
    let _ = app.emit(POPOVER_OPENED, current_anchor(app));
}

/// Shows a new client's popover, the first time this is called for it. A window
/// is only shown once its webview is up: WebView2 moves focus around as it
/// starts, which would read as a click elsewhere and dismiss the popover.
pub fn client_ready(app: &AppHandle) -> ClientState {
    let state = app.state::<TrayState>();
    let pending = state
        .pending
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .take();
    if let (Some(Pending::Show(placement)), Some(window)) = (pending, window(app)) {
        present(app, &window, placement);
    }
    ClientState {
        anchor: current_anchor(app),
        shown: window(app).is_some_and(|window| window.is_visible().unwrap_or(false)),
    }
}

fn current_anchor(app: &AppHandle) -> Option<Anchor> {
    *app.state::<TrayState>()
        .anchor
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Shows the popover beside the icon, starting a new client if there is none.
/// With no icon position given, it uses wherever the tray says the icon is.
pub fn open_window(app: &AppHandle, icon: Option<Rect>) {
    app.state::<TrayState>()
        .window_generation
        .fetch_add(1, Ordering::SeqCst);
    let placement = placement(app, icon);

    let existing = window(app);
    let state = app.state::<TrayState>();
    let mut pending = state
        .pending
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());

    if let (Some(window), None) = (&existing, &*pending) {
        drop(pending);
        present(app, window, placement);
        return;
    }

    // A client that is still starting shows itself as soon as it is ready.
    *pending = Some(Pending::Show(placement));
    drop(pending);
    if existing.is_none() {
        start_client(app);
    }
}

/// Creates the window hidden. `client_ready` decides what happens next.
fn start_client(app: &AppHandle) {
    // Creating a webview from an event handler deadlocks on Windows, because
    // both wait on the main thread. A separate task avoids that.
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let built = WebviewWindowBuilder::new(&app, WINDOW_LABEL, WebviewUrl::default())
            .title("Pulse")
            .inner_size(
                popover::PANEL_WIDTH + popover::MARGIN * 2.0,
                popover::PANEL_HEIGHT + popover::MARGIN * 2.0,
            )
            .visible(false)
            .decorations(false)
            .shadow(false)
            .transparent(true)
            .resizable(false)
            .always_on_top(true)
            .skip_taskbar(true)
            .build();
        match built {
            Ok(_) => {
                tokio::time::sleep(READY_TIMEOUT).await;
                client_ready(&app);
            }
            Err(e) => eprintln!("could not open popover: {e}"),
        }
    });
}

/// Fades the popover out and hides it, then shuts the client down after the
/// delay, unless it is reopened first or the user keeps it ready.
pub fn close_window(app: &AppHandle) {
    let Some(window) = window(app) else { return };
    if !window.is_visible().unwrap_or(false) {
        return;
    }

    let state = app.state::<TrayState>();
    *state
        .closed_at
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(Instant::now());
    let generation = state.window_generation.fetch_add(1, Ordering::SeqCst) + 1;
    let _ = app.emit(POPOVER_CLOSING, ());

    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let superseded = |app: &AppHandle| {
            app.state::<TrayState>()
                .window_generation
                .load(Ordering::SeqCst)
                != generation
        };

        tokio::time::sleep(FADE_OUT).await;
        if superseded(&app) {
            return;
        }
        let _ = window.hide();

        tokio::time::sleep(SHUTDOWN_DELAY).await;
        if !superseded(&app) && !keeps_client_ready(&app) {
            let _ = window.destroy();
        }
    });
}
