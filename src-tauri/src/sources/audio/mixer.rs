//! The apps making sound on the current output. Windows splits an app into many
//! sessions, one or more per process, and these are gathered back into one app.

use std::collections::HashMap;

use serde_json::{Value as Json, json};
use windows::{
    Win32::Media::Audio::{
        AudioSessionStateActive, AudioSessionStateExpired, Endpoints::IAudioMeterInformation,
        IAudioSessionControl2, IAudioSessionManager2, IMMDevice, ISimpleAudioVolume,
    },
    Win32::{Foundation::S_OK, System::Com::CLSCTX_ALL},
    core::{GUID, Interface, Result},
};

use super::capture::Capture;
use crate::sources::apps::Apps;

/// The most apps that get a waveform at once. Each one is a stream of its own.
const CAPTURE_LIMIT: usize = 6;

/// Below this an app is treated as silent, as sessions rarely read exactly zero.
const QUIET: f32 = 0.0005;

struct Session {
    process_id: u32,
    active: bool,
    volume: ISimpleAudioVolume,
    meter: IAudioMeterInformation,
}

struct Group {
    id: String,
    name: String,
    icon: Option<String>,
    sessions: Vec<Session>,
}

impl Group {
    fn peak(&self) -> f32 {
        self.sessions
            .iter()
            // SAFETY: a plain query on an object this value owns.
            .filter_map(|session| unsafe { session.meter.GetPeakValue() }.ok())
            .fold(0.0, f32::max)
    }
}

#[derive(Default)]
pub struct Mixer {
    apps: Apps,
    /// Every app seen, in order, so rows never jump while someone reaches for one.
    order: Vec<String>,
    groups: Vec<Group>,
    /// Each app's waveform, with the process it follows.
    captures: HashMap<String, (u32, Capture)>,
    /// Apps that can't be captured, so they aren't retried on every refresh.
    uncapturable: Vec<String>,
}

fn session_of(control: &IAudioSessionControl2) -> Result<Option<Session>> {
    // SAFETY: plain queries on a session Windows just handed over.
    unsafe {
        let state = control.GetState()?;
        // S_FALSE means it is not, and that also counts as success.
        if state == AudioSessionStateExpired || control.IsSystemSoundsSession() == S_OK {
            return Ok(None);
        }
        Ok(Some(Session {
            process_id: control.GetProcessId()?,
            active: state == AudioSessionStateActive,
            volume: control.cast()?,
            meter: control.cast()?,
        }))
    }
}

impl Mixer {
    /// Reads every app on a device, and starts or stops their waveforms to match.
    pub fn refresh(&mut self, device: &IMMDevice) -> Result<Json> {
        // SAFETY: plain queries over a collection owned by this call.
        let sessions = unsafe {
            let manager: IAudioSessionManager2 = device.Activate(CLSCTX_ALL, None)?;
            let list = manager.GetSessionEnumerator()?;
            let mut sessions = Vec::new();
            for index in 0..list.GetCount()? {
                let control: IAudioSessionControl2 = list.GetSession(index)?.cast()?;
                sessions.extend(session_of(&control).ok().flatten());
            }
            sessions
        };

        let mut groups: Vec<Group> = Vec::new();
        for session in sessions {
            if session.process_id == std::process::id() {
                continue;
            }
            let Some(app) = self.apps.of_process(session.process_id).cloned() else {
                continue;
            };
            match groups.iter_mut().find(|group| group.id == app.id) {
                Some(group) => group.sessions.push(session),
                None => groups.push(Group {
                    id: app.id,
                    name: app.name,
                    icon: app.icon,
                    sessions: vec![session],
                }),
            }
        }

        for group in &groups {
            if !self.order.contains(&group.id) {
                self.order.push(group.id.clone());
            }
        }
        groups.sort_by_key(|group| self.order.iter().position(|id| *id == group.id));
        self.groups = groups;
        self.follow_sound();
        self.describe()
    }

    fn follow_sound(&mut self) {
        let playing: Vec<(String, u32)> = self
            .groups
            .iter()
            .filter_map(|group| {
                let session = group.sessions.iter().find(|session| session.active)?;
                Some((group.id.clone(), session.process_id))
            })
            .take(CAPTURE_LIMIT)
            .collect();

        // An app's sound can move between its processes, as Chrome's does.
        self.captures.retain(|id, (process_id, _)| {
            playing
                .iter()
                .any(|playing| playing.0 == *id && playing.1 == *process_id)
        });
        for (id, process_id) in playing {
            if self.captures.contains_key(&id) || self.uncapturable.contains(&id) {
                continue;
            }
            match Capture::of_process(process_id) {
                Ok(capture) => {
                    self.captures.insert(id, (process_id, capture));
                }
                Err(_) => self.uncapturable.push(id),
            }
        }
    }

    fn describe(&self) -> Result<Json> {
        self.groups
            .iter()
            .map(|group| {
                let first = group
                    .sessions
                    .first()
                    .ok_or_else(windows::core::Error::empty)?;
                // SAFETY: plain queries on objects this value owns.
                let (volume, muted) = unsafe {
                    (
                        first.volume.GetMasterVolume()?,
                        first.volume.GetMute()?.as_bool(),
                    )
                };
                Ok(json!({
                    "id": group.id,
                    "name": group.name,
                    "icon": group.icon,
                    "volume": volume,
                    "muted": muted,
                    "playing": group.sessions.iter().any(|session| session.active),
                }))
            })
            .collect::<Result<Vec<_>>>()
            .map(Json::from)
    }

    /// One moment of every app's sound.
    pub fn frame(&mut self) -> Json {
        // A stream dies across sleep, and the next refresh starts a new one.
        self.captures
            .retain(|_, (_, capture)| capture.read().is_ok());

        let moments: Vec<Json> = self
            .groups
            .iter()
            .map(|group| {
                let peak = group.peak();
                json!({
                    "id": group.id,
                    "peak": if peak < QUIET { 0.0 } else { peak },
                    "bands": self.captures.get(&group.id).map(|(_, capture)| capture.bands()),
                })
            })
            .collect();
        Json::from(moments)
    }

    /// Lets go of everything, as nobody is looking at the tab.
    pub fn stop(&mut self) {
        self.captures.clear();
        self.groups.clear();
        self.uncapturable.clear();
    }

    fn sessions_of(&self, id: &str) -> std::result::Result<&[Session], String> {
        self.groups
            .iter()
            .find(|group| group.id == id)
            .map(|group| group.sessions.as_slice())
            .ok_or_else(|| "That app has closed".to_owned())
    }

    pub fn set_volume(&self, id: &str, value: f32) -> std::result::Result<(), String> {
        for session in self.sessions_of(id)? {
            // SAFETY: a plain call on an object this value owns.
            unsafe {
                session
                    .volume
                    .SetMasterVolume(value.clamp(0.0, 1.0), &GUID::zeroed())
            }
            .map_err(|e| format!("Could not set the volume: {e}"))?;
        }
        Ok(())
    }

    pub fn set_muted(&self, id: &str, muted: bool) -> std::result::Result<(), String> {
        for session in self.sessions_of(id)? {
            // SAFETY: a plain call on an object this value owns.
            unsafe { session.volume.SetMute(muted, &GUID::zeroed()) }
                .map_err(|e| format!("Could not change mute: {e}"))?;
        }
        Ok(())
    }
}
