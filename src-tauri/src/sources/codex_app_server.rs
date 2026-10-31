//! A client for Codex's app server, which reads usage with Codex's own login.
//!
//! The server is not a service that stays running. Each read starts one with
//! `codex app-server`, talks to it over stdio, and ends it. Pulse never opens the
//! Codex login, and only ever calls methods that read.

use std::{
    env, fs,
    path::{Path, PathBuf},
    process::Stdio,
};

use serde::de::DeserializeOwned;
use serde_json::{Value as Json, json};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader, Lines},
    process::{Child, ChildStdin, ChildStdout, Command},
};

use super::CREATE_NO_WINDOW;

pub struct AppServer {
    /// Held so that the server ends when this is dropped, even mid-request.
    _process: Child,
    input: ChildStdin,
    output: Lines<BufReader<ChildStdout>>,
    next_id: u64,
}

impl AppServer {
    pub async fn start(codex: &Path) -> Result<Self, String> {
        let mut process = Command::new(codex)
            .arg("app-server")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .creation_flags(CREATE_NO_WINDOW)
            .kill_on_drop(true)
            .spawn()
            .map_err(|e| format!("Could not start Codex: {e}"))?;

        let input = process.stdin.take().ok_or("Could not write to Codex")?;
        let output = process.stdout.take().ok_or("Could not read from Codex")?;

        let mut server = Self {
            _process: process,
            input,
            output: BufReader::new(output).lines(),
            next_id: 0,
        };
        let client_info =
            json!({ "name": "pulse", "title": "Pulse", "version": env!("CARGO_PKG_VERSION") });
        server
            .request::<Json>("initialize", json!({ "clientInfo": client_info }))
            .await?;
        server.send(&json!({ "method": "initialized" })).await?;
        Ok(server)
    }

    async fn send(&mut self, message: &Json) -> Result<(), String> {
        self.input
            .write_all(format!("{message}\n").as_bytes())
            .await
            .map_err(|e| format!("Could not write to Codex: {e}"))
    }

    pub async fn request<T: DeserializeOwned>(
        &mut self,
        method: &str,
        params: Json,
    ) -> Result<T, String> {
        self.next_id += 1;
        let id = self.next_id;
        self.send(&json!({ "method": method, "id": id, "params": params }))
            .await?;

        loop {
            let line = self
                .output
                .next_line()
                .await
                .map_err(|e| format!("Could not read from Codex: {e}"))?
                .ok_or("Codex closed before it answered")?;

            // The server also sends notifications, which carry no matching id.
            let Ok(reply) = serde_json::from_str::<Json>(&line) else {
                continue;
            };
            if reply.get("id").and_then(Json::as_u64) != Some(id) {
                continue;
            }
            if let Some(error) = reply.get("error") {
                let message = error.get("message").and_then(Json::as_str);
                return Err(format!(
                    "Codex said: {}",
                    message.unwrap_or("unknown error")
                ));
            }
            let result = reply.get("result").cloned().unwrap_or(Json::Null);
            return serde_json::from_value(result)
                .map_err(|e| format!("Could not understand Codex's reply to {method}: {e}"));
        }
    }
}

/// The installed Codex, trying the desktop app first and then the command line.
pub async fn find_codex() -> Option<PathBuf> {
    match desktop_codex() {
        Some(codex) => Some(codex),
        None => command_line_codex().await,
    }
}

fn subfolders(folder: &Path) -> Vec<PathBuf> {
    fs::read_dir(folder).map_or_else(
        |_| vec![],
        |entries| {
            entries
                .flatten()
                .map(|entry| entry.path())
                .filter(|path| path.is_dir())
                .collect()
        },
    )
}

/// The desktop app keeps each version in its own folder, so the newest one is
/// the one in use.
fn desktop_codex() -> Option<PathBuf> {
    let versions = Path::new(&env::var_os("LOCALAPPDATA")?).join("OpenAI/Codex/bin");
    subfolders(&versions)
        .into_iter()
        .map(|version| version.join("codex.exe"))
        .filter(|codex| codex.exists())
        .max_by_key(|codex| fs::metadata(codex).and_then(|file| file.modified()).ok())
}

/// `where` finds the npm script rather than the program it runs, and ending a
/// script would leave that program behind, so this finds the program itself.
async fn command_line_codex() -> Option<PathBuf> {
    let found = Command::new("where.exe")
        .arg("codex")
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .await
        .ok()?;

    String::from_utf8_lossy(&found.stdout)
        .lines()
        .map(|line| PathBuf::from(line.trim()))
        .find_map(|path| {
            if path.extension().is_some_and(|extension| extension == "exe") {
                Some(path)
            } else {
                npm_package_codex(path.parent()?)
            }
        })
}

fn npm_package_codex(npm_folder: &Path) -> Option<PathBuf> {
    let packages = npm_folder.join("node_modules/@openai/codex/node_modules/@openai");
    subfolders(&packages)
        .into_iter()
        .filter(|package| {
            package
                .file_name()
                .is_some_and(|name| name.to_string_lossy().starts_with("codex-win32"))
        })
        .flat_map(|package| subfolders(&package.join("vendor")))
        .map(|platform| platform.join("bin/codex.exe"))
        .find(|codex| codex.exists())
}
