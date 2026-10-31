//! Connects Pulse to Claude Code's status line, the one documented place that
//! exposes subscription limits.
//!
//! Connecting points `statusLine` in Claude Code's settings at this executable.
//! Claude Code then runs it with a JSON payload on stdin, and `run_hook` saves
//! only the `rate_limits` part. A status line the user already had is kept in
//! `ORIGINAL_FILE`, still runs through the hook, and is restored on disconnect.

use std::{
    env, fs,
    io::{Read, Write},
    os::windows::process::CommandExt,
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

use serde_json::{Map, Value as Json, json};

use super::{CREATE_NO_WINDOW, Setup, SetupState, Terms, data_file, path_folders};

/// Passed by Claude Code to run this executable as the hook instead of the app.
pub const HOOK_FLAG: &str = "--claude-status";

const STATUS_FILE: &str = "claude-status.json";
const ORIGINAL_FILE: &str = "claude-status-line-original.json";
const BACKUP_SUFFIX: &str = ".pulse-backup";
const STATUS_LINE: &str = "statusLine";

const TERMS: Terms = Terms {
    will: &[
        "Add one command to Claude Code's settings, after backing them up",
        "Save your limits each time Claude Code responds",
        "Read your transcripts to count tokens by model",
        "Keep those figures on this computer",
    ],
    never: &[
        "See or store your Claude login",
        "Keep the text of your conversations",
        "Send anything off this computer",
    ],
    note: "Your existing status line keeps working. Disconnect at any time in Settings to \
           restore your settings and clear these figures.",
};

pub fn claude_dir() -> Option<PathBuf> {
    if let Some(dir) = env::var_os("CLAUDE_CONFIG_DIR") {
        return Some(PathBuf::from(dir));
    }
    Some(Path::new(&env::var_os("USERPROFILE")?).join(".claude"))
}

pub fn status_file() -> Option<PathBuf> {
    data_file(STATUS_FILE)
}

fn settings_path() -> Result<PathBuf, String> {
    claude_dir()
        .map(|dir| dir.join("settings.json"))
        .ok_or_else(|| "Could not find the Claude Code folder".to_owned())
}

/// Forward slashes and quotes keep the path intact through any shell.
fn hook_command() -> Result<String, String> {
    let exe = env::current_exe().map_err(|e| format!("Could not find Pulse: {e}"))?;
    Ok(format!(
        "\"{}\" {HOOK_FLAG}",
        exe.to_string_lossy().replace('\\', "/")
    ))
}

fn command_of(status_line: &Json) -> Option<&str> {
    status_line.get("command")?.as_str()
}

fn read_settings(path: &Path) -> Result<Map<String, Json>, String> {
    let text = match fs::read_to_string(path) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Map::new()),
        Err(e) => return Err(format!("Could not read Claude Code settings: {e}")),
    };
    // A file that cannot be understood is left alone rather than replaced.
    serde_json::from_str(&text)
        .map_err(|e| format!("Claude Code settings could not be read, so nothing changed: {e}"))
}

/// Replaces a file in one step, so a reader never sees half of it.
fn write_atomically(path: &Path, contents: &[u8]) -> std::io::Result<()> {
    let temporary = path.with_extension("pulse-tmp");
    fs::write(&temporary, contents)?;
    fs::rename(&temporary, path)
}

fn write_settings(path: &Path, settings: &Map<String, Json>) -> Result<(), String> {
    let text = serde_json::to_string_pretty(settings).map_err(|e| e.to_string())?;
    write_atomically(path, text.as_bytes())
        .map_err(|e| format!("Could not save Claude Code settings: {e}"))
}

pub fn setup() -> Setup {
    let state = current_state();
    let (title, detail) = match state {
        SetupState::Unavailable => (
            "Claude Code isn't installed",
            "Install Claude Code to see your usage here.",
        ),
        SetupState::Disconnected => (
            "Connect Claude Code",
            "Pulse reads your usage from what Claude Code keeps on this computer.",
        ),
        SetupState::Waiting => (
            "Connected to Claude Code",
            "Your limits appear after Claude Code's next response.",
        ),
        SetupState::Connected => (
            "Claude Code",
            "Limits update while Claude Code is responding.",
        ),
    };
    Setup::new(state, title, detail, TERMS)
}

/// Whether the user has given Pulse permission to read Claude Code's files.
pub fn is_connected() -> bool {
    matches!(current_state(), SetupState::Waiting | SetupState::Connected)
}

fn current_state() -> SetupState {
    if !claude_dir().is_some_and(|dir| dir.is_dir()) {
        return SetupState::Unavailable;
    }
    let connected = settings_path()
        .and_then(|path| read_settings(&path))
        .ok()
        .and_then(|settings| settings.get(STATUS_LINE).cloned())
        .is_some_and(|status_line| {
            // A different path means Pulse moved, so the entry needs refreshing.
            command_of(&status_line).map(str::to_owned) == hook_command().ok()
        });

    match (connected, status_file().is_some_and(|file| file.exists())) {
        (false, _) => SetupState::Disconnected,
        (true, false) => SetupState::Waiting,
        (true, true) => SetupState::Connected,
    }
}

pub fn connect() -> Result<(), String> {
    let path = settings_path()?;
    let mut settings = read_settings(&path)?;
    let existing = settings.get(STATUS_LINE).cloned();
    let ours = existing
        .as_ref()
        .and_then(command_of)
        .is_some_and(|command| command.contains(HOOK_FLAG));

    if path.exists() {
        let backup = path.with_extension(format!("json{BACKUP_SUFFIX}"));
        fs::copy(&path, backup).map_err(|e| format!("Could not back up settings: {e}"))?;
    }

    // Keep any other fields, such as padding, on the entry being wrapped.
    let mut status_line = existing.clone().unwrap_or_else(|| json!({}));
    if let (Some(existing), false) = (&existing, ours) {
        let original = data_file(ORIGINAL_FILE).ok_or("Could not find Pulse's data folder")?;
        fs::write(original, existing.to_string())
            .map_err(|e| format!("Could not save your status line: {e}"))?;
    }
    status_line["type"] = json!("command");
    status_line["command"] = json!(hook_command()?);

    settings.insert(STATUS_LINE.to_owned(), status_line);
    write_settings(&path, &settings)
}

pub fn disconnect() -> Result<(), String> {
    let path = settings_path()?;
    let mut settings = read_settings(&path)?;
    let original_file = data_file(ORIGINAL_FILE);
    let original = original_file
        .as_ref()
        .and_then(|file| fs::read(file).ok())
        .and_then(|bytes| serde_json::from_slice::<Json>(&bytes).ok());

    match original {
        Some(original) => {
            settings.insert(STATUS_LINE.to_owned(), original);
        }
        None => {
            settings.remove(STATUS_LINE);
        }
    }
    write_settings(&path, &settings)?;

    for file in [original_file, status_file()].into_iter().flatten() {
        let _ = fs::remove_file(file);
    }
    Ok(())
}

/// Runs as Claude Code's status line command. Failures stay silent: a problem
/// here must never disturb Claude Code.
pub fn run_hook() {
    let mut payload = Vec::new();
    if std::io::stdin().read_to_end(&mut payload).is_err() {
        return;
    }

    save_rate_limits(&payload);
    if let Some(output) = run_original(&payload) {
        let _ = std::io::stdout().write_all(&output);
    }
}

/// The rest of the payload names folders and sessions, and Pulse has no use for it.
fn save_rate_limits(payload: &[u8]) {
    let Ok(status) = serde_json::from_slice::<Json>(payload) else {
        return;
    };
    let Some(limits) = status.get("rate_limits").filter(|limits| !limits.is_null()) else {
        return;
    };
    if let Some(file) = status_file() {
        let saved = json!({ "rate_limits": limits }).to_string();
        let _ = write_atomically(&file, saved.as_bytes());
    }
}

fn run_original(payload: &[u8]) -> Option<Vec<u8>> {
    let original: Json = serde_json::from_slice(&fs::read(data_file(ORIGINAL_FILE)?).ok()?).ok()?;
    let command = command_of(&original)?;

    let mut child = shell(command)
        .creation_flags(CREATE_NO_WINDOW)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    child.stdin.take()?.write_all(payload).ok()?;
    Some(child.wait_with_output().ok()?.stdout)
}

/// The first Git Bash on PATH. System32's bash is WSL, which cannot see Windows
/// paths the same way.
fn bash_on_path() -> Option<PathBuf> {
    path_folders()
        .into_iter()
        .filter(|folder| !folder.to_string_lossy().to_lowercase().contains("system32"))
        .map(|folder| folder.join("bash.exe"))
        .find(|bash| bash.exists())
}

/// Status lines are written for Git Bash, so it is used when it is there.
fn shell(command: &str) -> Command {
    let bash = env::var_os("CLAUDE_CODE_GIT_BASH_PATH")
        .map(PathBuf::from)
        .filter(|path| path.exists())
        .or_else(bash_on_path);

    match bash {
        Some(bash) => {
            let mut shell = Command::new(bash);
            shell.arg("-c").arg(command);
            shell
        }
        None => {
            let mut shell = Command::new(env::var_os("ComSpec").unwrap_or_else(|| "cmd".into()));
            shell.arg("/C").arg(command);
            shell
        }
    }
}
