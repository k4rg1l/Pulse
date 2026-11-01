//! What is playing, as any app that reports to Windows' media controls describes it.

use std::time::{SystemTime, UNIX_EPOCH};

use base64::{Engine, engine::general_purpose::STANDARD};
use serde_json::{Value as Json, json};
use windows::{
    Media::Control::{
        GlobalSystemMediaTransportControlsSession as MediaSession,
        GlobalSystemMediaTransportControlsSessionManager as MediaManager,
        GlobalSystemMediaTransportControlsSessionPlaybackStatus as PlaybackStatus,
    },
    Storage::Streams::{DataReader, IRandomAccessStreamReference},
    core::Result,
};

use super::Transport;

/// Windows counts time in 100 nanosecond ticks.
const TICKS_PER_SECOND: f64 = 10_000_000.0;

/// Seconds from Windows' epoch, in 1601, to the Unix one.
const EPOCH_GAP_SECONDS: f64 = 11_644_473_600.0;

/// Reading artwork is slow, so it is kept until the track changes.
#[derive(Default)]
struct Artwork {
    track: String,
    url: Option<String>,
}

pub struct Media {
    manager: MediaManager,
    artwork: Artwork,
}

fn image_url(reference: &IRandomAccessStreamReference) -> Result<String> {
    let stream = reference.OpenReadAsync()?.join()?;
    let size = stream.Size()? as u32;
    let kind = stream.ContentType()?.to_string();
    let reader = DataReader::CreateDataReader(&stream)?;
    reader.LoadAsync(size)?.join()?;
    let mut bytes = vec![0u8; size as usize];
    reader.ReadBytes(&mut bytes)?;
    let kind = if kind.starts_with("image/") {
        kind.as_str()
    } else {
        "image/jpeg"
    };
    Ok(format!("data:{kind};base64,{}", STANDARD.encode(bytes)))
}

/// The program behind a media session, from names like "Spotify.exe" or
/// "SpotifyAB.SpotifyMusic_zpdnekdrzrea0!Spotify".
fn app_of(source: &str) -> String {
    let name = source.rsplit('!').next().unwrap_or(source);
    name.split('.').next().unwrap_or(name).to_lowercase()
}

fn now_seconds() -> f64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0.0, |since| since.as_secs_f64())
        + EPOCH_GAP_SECONDS
}

impl Media {
    pub fn new() -> Result<Self> {
        Ok(Self {
            manager: MediaManager::RequestAsync()?.join()?,
            artwork: Artwork::default(),
        })
    }

    fn session(&self) -> Option<MediaSession> {
        self.manager.GetCurrentSession().ok()
    }

    pub fn read(&mut self) -> Json {
        self.session()
            .and_then(|session| self.describe(&session).ok())
            .unwrap_or(Json::Null)
    }

    fn describe(&mut self, session: &MediaSession) -> Result<Json> {
        let properties = session.TryGetMediaPropertiesAsync()?.join()?;
        let title = properties.Title()?.to_string();
        let artist = properties.Artist()?.to_string();
        let playing = session.GetPlaybackInfo()?.PlaybackStatus()? == PlaybackStatus::Playing;

        let track = format!("{title}\n{artist}");
        if track != self.artwork.track {
            self.artwork = Artwork {
                url: properties
                    .Thumbnail()
                    .ok()
                    .and_then(|image| image_url(&image).ok()),
                track,
            };
        }

        let timeline = session.GetTimelineProperties()?;
        let duration = timeline.EndTime()?.Duration as f64 / TICKS_PER_SECOND;
        let reported = timeline.Position()?.Duration as f64 / TICKS_PER_SECOND;
        let reported_at = timeline.LastUpdatedTime()?.UniversalTime as f64 / TICKS_PER_SECOND;
        // Apps report their position only now and then, so it is carried forward.
        let position = if playing {
            reported + (now_seconds() - reported_at).max(0.0)
        } else {
            reported
        };

        Ok(json!({
            "title": title,
            "artist": artist,
            "app": app_of(&session.SourceAppUserModelId()?.to_string()),
            "art": self.artwork.url,
            "playing": playing,
            "position": if duration > 0.0 { position.min(duration) } else { 0.0 },
            "duration": duration,
        }))
    }

    pub fn perform(&self, command: Transport) -> std::result::Result<(), String> {
        let session = self.session().ok_or("Nothing is playing")?;
        let sent = match command {
            Transport::Previous => session.TrySkipPreviousAsync(),
            Transport::Toggle => session.TryTogglePlayPauseAsync(),
            Transport::Next => session.TrySkipNextAsync(),
        };
        sent.and_then(|operation| operation.join())
            .map(|_| ())
            .map_err(|e| format!("Could not control playback: {e}"))
    }
}
