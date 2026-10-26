//! The state model Mira renders.
//!
//! Times are kept as microseconds (`i64`) to match both MPRIS and the D-Bus
//! surface exactly, avoiding float drift. Runtime-only fields that make no
//! sense to persist or serialize over the wire are marked `#[serde(skip)]`.

use serde::{Deserialize, Serialize};
use std::time::Instant;

/// One microsecond's worth of `i64` ticks, for readability at call sites.
pub const MICROS_PER_SEC: i64 = 1_000_000;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub enum PlaybackStatus {
    #[default]
    Stopped,
    Playing,
    Paused,
}

impl PlaybackStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            PlaybackStatus::Stopped => "Stopped",
            PlaybackStatus::Playing => "Playing",
            PlaybackStatus::Paused => "Paused",
        }
    }

    pub fn from_mpris(s: &str) -> Self {
        match s {
            "Playing" => PlaybackStatus::Playing,
            "Paused" => PlaybackStatus::Paused,
            _ => PlaybackStatus::Stopped,
        }
    }

    pub fn is_playing(self) -> bool {
        matches!(self, PlaybackStatus::Playing)
    }
}

/// What the media source can do, surfaced so the UI can enable/disable buttons.
///
/// The derived default is all-`false`: assume nothing works until a source
/// reports otherwise.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct MediaCapabilities {
    pub can_play: bool,
    pub can_pause: bool,
    pub can_go_next: bool,
    pub can_go_previous: bool,
    pub can_seek: bool,
}

/// The currently playing track, as reported by the active media source.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct NowPlaying {
    pub status: PlaybackStatus,
    pub title: String,
    pub artist: String,
    pub album: String,
    /// Local filesystem path to cached cover art, if any.
    pub art_path: Option<String>,
    pub length_micros: i64,
    /// Position at the moment it was sampled; combine with [`Self::sampled_at`]
    /// to extrapolate the live position while playing, without polling.
    pub position_micros: i64,
    pub caps: MediaCapabilities,
    /// Opaque identifier of the source player (e.g. MPRIS bus name). Used to
    /// detect player switches. Not shown to the user.
    pub player_id: String,

    /// Monotonic instant at which `position_micros` was sampled. Runtime only.
    #[serde(skip)]
    pub sampled_at: Option<Instant>,
}

impl NowPlaying {
    /// Live position, extrapolated from the last sample while playing.
    /// Clamped to `[0, length_micros]` when a length is known.
    pub fn live_position_micros(&self) -> i64 {
        let mut pos = self.position_micros;
        if self.status.is_playing() {
            if let Some(t) = self.sampled_at {
                pos += t.elapsed().as_micros() as i64;
            }
        }
        if self.length_micros > 0 {
            pos.clamp(0, self.length_micros)
        } else {
            pos.max(0)
        }
    }

    /// True when there is a meaningful track to show.
    pub fn has_track(&self) -> bool {
        !self.title.is_empty() || !self.artist.is_empty()
    }
}

/// Battery / charger state.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub enum PowerState {
    #[default]
    Unknown,
    Charging,
    Discharging,
    FullyCharged,
    Empty,
}

impl PowerState {
    pub fn as_str(self) -> &'static str {
        match self {
            PowerState::Unknown => "Unknown",
            PowerState::Charging => "Charging",
            PowerState::Discharging => "Discharging",
            PowerState::FullyCharged => "FullyCharged",
            PowerState::Empty => "Empty",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Power {
    pub has_battery: bool,
    /// 0.0 – 100.0
    pub percentage: f64,
    pub state: PowerState,
    pub on_ac: bool,
}

/// Transient system events that trigger a short notch "alert" expansion.
/// Phase 1 only emits the power variants; the rest are declared for phase 2.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum SystemEvent {
    ChargerConnected { percentage: f64 },
    ChargerDisconnected { percentage: f64 },
    // --- phase 2 ---
    WifiLost,
    WifiBack { ssid: String },
    HeadphonesConnected { name: String, battery: Option<f64> },
    VolumeChanged { level: f64, muted: bool },
    BrightnessChanged { level: f64 },
    CapsLock { on: bool },
    KeyboardLanguage { code: String },
}

// --- Phase 2 feature state (declared now so the model is complete) ---

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PomodoroPhase {
    Work,
    ShortBreak,
    LongBreak,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Pomodoro {
    pub phase: PomodoroPhase,
    pub remaining_secs: i64,
    pub running: bool,
    pub completed_cycles: u32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Water {
    pub glasses_today: u32,
    pub goal: u32,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Todo {
    pub id: i64,
    pub text: String,
    pub done: bool,
    pub remind_at_epoch: Option<i64>,
}

/// A shelf entry stores only the *path* to a file, never its contents.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ShelfItem {
    pub id: i64,
    pub path: String,
    pub added_epoch: i64,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Shelf {
    pub items: Vec<ShelfItem>,
}
