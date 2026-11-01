//! The one thread that talks to Windows audio. Its objects have threading rules of
//! their own, so they stay here, away from the async runtime, and everything else
//! reaches them through a queue.
//!
//! Costly work follows the panel. Apps, tracks and waveforms are read only while
//! someone is looking at the tab, and all of it stops when they aren't.

use std::{
    sync::{
        Arc, Mutex,
        mpsc::{self, Receiver, RecvTimeoutError, Sender},
    },
    thread,
    time::{Duration, Instant},
};

use serde_json::{Value as Json, json};
use tokio::sync::Notify;
use windows::Win32::System::Com::{COINIT_MULTITHREADED, CoInitializeEx};

use super::{Action, Flow, capture::Capture, devices::Devices, media::Media, mixer::Mixer};
use crate::sources::LiveSink;

/// How long the user's action may wait on Windows before it counts as failed.
const ACTION_TIMEOUT: Duration = Duration::from_secs(3);

/// How often a line is sent. Matches FRAME_MS in src/audio/LivingFader.tsx.
const FRAME: Duration = Duration::from_millis(33);

/// How often apps and the track are read again while the tab is showing.
const REFRESH: Duration = Duration::from_secs(1);

pub enum Command {
    Refresh,
    Perform(Action, Sender<Result<(), String>>),
    Live(Option<LiveSink>),
    Stop,
}

type State = Arc<Mutex<Result<Json, String>>>;

pub struct Worker {
    commands: Sender<Command>,
    state: State,
}

impl Worker {
    /// Starts the thread. `changed` is told whenever what Pulse knows has changed.
    pub fn start(changed: Arc<Notify>) -> Self {
        let (commands, queue) = mpsc::channel();
        let state: State = Arc::new(Mutex::new(Err("Audio is starting".to_owned())));

        let events = commands.clone();
        let shared = state.clone();
        let started = thread::Builder::new()
            .name("audio".to_owned())
            .spawn(move || run(queue, events, shared, changed));
        if let Err(e) = started {
            eprintln!("could not start audio: {e}");
        }

        Self { commands, state }
    }

    pub fn state(&self) -> Result<Json, String> {
        self.state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone()
    }

    pub fn refresh(&self) {
        let _ = self.commands.send(Command::Refresh);
    }

    /// Ends the thread, which releases everything it holds from Windows.
    pub fn stop(&self) {
        let _ = self.commands.send(Command::Stop);
    }

    pub fn set_live(&self, sink: Option<LiveSink>) {
        let _ = self.commands.send(Command::Live(sink));
    }

    pub fn perform(&self, action: Action) -> Result<(), String> {
        let (reply, result) = mpsc::channel();
        self.commands
            .send(Command::Perform(action, reply))
            .map_err(|_| "Audio has stopped".to_owned())?;
        result
            .recv_timeout(ACTION_TIMEOUT)
            .map_err(|_| "Windows did not respond".to_owned())?
    }
}

/// Everything read from Windows on this thread.
struct Engine {
    devices: Devices,
    mixer: Mixer,
    media: Option<Media>,
    output: Option<Capture>,
    /// The device everything live is attached to.
    following: Option<String>,
    apps: Json,
    track: Json,
}

impl Engine {
    fn new(events: Sender<Command>) -> Result<Self, String> {
        Ok(Self {
            devices: Devices::new(events).map_err(|e| format!("Could not read audio: {e}"))?,
            mixer: Mixer::default(),
            media: Media::new().ok(),
            output: None,
            following: None,
            apps: json!([]),
            track: Json::Null,
        })
    }

    fn perform(&self, action: Action) -> Result<(), String> {
        match action {
            Action::SetAppVolume { app, value } => self.mixer.set_volume(&app, value),
            Action::SetAppMuted { app, muted } => self.mixer.set_muted(&app, muted),
            Action::Transport { command } => self
                .media
                .as_ref()
                .ok_or_else(|| "Media controls are unavailable".to_owned())?
                .perform(command),
            action => self.devices.perform(action),
        }
    }

    /// Reads apps and the track again, for an open tab.
    fn refresh_live(&mut self) {
        let current = self.devices.current_id(Flow::Output);
        if current != self.following {
            self.stop_live();
            self.following = current;
        }
        if let Some(device) = self.devices.default_device(Flow::Output) {
            self.apps = self.mixer.refresh(&device).unwrap_or_else(|_| json!([]));
            if self.output.is_none() {
                self.output = Capture::of_device(&device).ok();
            }
        }
        if let Some(media) = &mut self.media {
            self.track = media.read();
        }
    }

    fn snapshot(&mut self) -> Result<Json, String> {
        let mut snapshot = self.devices.snapshot()?;
        snapshot["apps"] = self.apps.clone();
        snapshot["track"] = self.track.clone();
        Ok(snapshot)
    }

    fn frame(&mut self) -> Json {
        // A stream dies with its device, and the next refresh starts a new one.
        let bands = self.output.as_mut().and_then(|capture| {
            capture.read().ok()?;
            Some(capture.bands())
        });
        if bands.is_none() {
            self.output = None;
        }
        json!({
            "bands": bands,
            "peak": self.devices.peak(Flow::Output),
            "input": self.devices.peak(Flow::Input),
            "apps": self.mixer.frame(),
        })
    }

    fn stop_live(&mut self) {
        self.output = None;
        self.mixer.stop();
    }
}

fn run(queue: Receiver<Command>, events: Sender<Command>, state: State, changed: Arc<Notify>) {
    let publish = |next: Result<Json, String>| {
        let mut current = state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if *current != next {
            *current = next;
            changed.notify_one();
        }
    };

    // SAFETY: called once, on this thread, before it uses any audio object.
    if let Err(e) = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) }.ok() {
        publish(Err(format!("Could not start audio: {e}")));
        return;
    }

    let mut engine = None;
    let mut sink: Option<LiveSink> = None;
    let mut refreshed = Instant::now();

    loop {
        let command = if sink.is_some() {
            queue.recv_timeout(FRAME)
        } else {
            queue.recv().map_err(|_| RecvTimeoutError::Disconnected)
        };

        let frame_only = matches!(command, Err(RecvTimeoutError::Timeout));

        // Also how Pulse recovers if the Windows audio service restarts.
        if engine.is_none() {
            match Engine::new(events.clone()) {
                Ok(started) => engine = Some(started),
                Err(e) => publish(Err(e)),
            }
        }

        match command {
            Ok(Command::Refresh) => {}
            Ok(Command::Perform(action, reply)) => {
                let result = engine.as_ref().map_or_else(
                    || Err("Audio has stopped".to_owned()),
                    |e| e.perform(action),
                );
                let _ = reply.send(result);
            }
            Ok(Command::Live(next)) => {
                sink = next;
                if let Some(engine) = &mut engine {
                    if sink.is_some() {
                        engine.refresh_live();
                    } else {
                        engine.stop_live();
                    }
                }
            }
            Ok(Command::Stop) | Err(RecvTimeoutError::Disconnected) => return,
            Err(RecvTimeoutError::Timeout) => {}
        }

        let Some(running) = &mut engine else { continue };

        let refresh_due = sink.is_some() && refreshed.elapsed() >= REFRESH;
        if refresh_due {
            refreshed = Instant::now();
            running.refresh_live();
        }
        if let Some(send) = &sink {
            if !send(running.frame()) {
                sink = None;
                running.stop_live();
            }
        }

        // A frame on its own changes nothing the tab reads from the snapshot.
        if frame_only && !refresh_due {
            continue;
        }
        let snapshot = running.snapshot();
        let failed = snapshot.is_err();
        publish(snapshot);
        if failed {
            engine = None;
        }
    }
}
