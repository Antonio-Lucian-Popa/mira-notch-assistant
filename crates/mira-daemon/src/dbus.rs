//! The D-Bus surface: `ro.novabytecode.Mira.{Service,Media,Lyrics,Power,System}`
//! served at `/ro/novabytecode/Mira` on the session bus.
//!
//! Every interface is a thin view over the shared [`AppState`]; property
//! getters read it, methods drive the active player, and the daemon's event
//! loop (see `main.rs`) mutates the state and fires the `*_changed` /signal
//! notifications. UIs (the GNOME extension) hold no business logic — they read
//! properties and react to signals.

use std::sync::Arc;

use tokio::sync::Mutex;
use zbus::object_server::SignalEmitter;

use mira_core::app::AppState;
use mira_core::source::MediaControl;

pub const PATH: &str = "/ro/novabytecode/Mira";
pub const BUS_NAME: &str = "ro.novabytecode.Mira";

/// State shared by all interfaces and the event loop.
pub struct Shared {
    pub state: Mutex<AppState>,
    pub control: Arc<dyn MediaControl>,
}

fn to_fdo(e: mira_core::Error) -> zbus::fdo::Error {
    zbus::fdo::Error::Failed(e.to_string())
}

// --- Service ---------------------------------------------------------------

pub struct ServiceIface {
    pub version: String,
    pub locale: String,
}

#[zbus::interface(name = "ro.novabytecode.Mira.Service")]
impl ServiceIface {
    /// Health check so a UI can tell whether the daemon is alive.
    async fn ping(&self) -> String {
        "pong".to_string()
    }

    #[zbus(property)]
    async fn version(&self) -> String {
        self.version.clone()
    }

    #[zbus(property)]
    async fn locale(&self) -> String {
        self.locale.clone()
    }
}

// --- Media -----------------------------------------------------------------

pub struct MediaIface {
    pub shared: Arc<Shared>,
}

#[zbus::interface(name = "ro.novabytecode.Mira.Media")]
impl MediaIface {
    async fn play_pause(&self) -> zbus::fdo::Result<()> {
        self.shared.control.play_pause().await.map_err(to_fdo)
    }
    async fn next(&self) -> zbus::fdo::Result<()> {
        self.shared.control.next().await.map_err(to_fdo)
    }
    async fn previous(&self) -> zbus::fdo::Result<()> {
        self.shared.control.previous().await.map_err(to_fdo)
    }
    /// Relative seek in microseconds (may be negative).
    async fn seek(&self, offset_micros: i64) -> zbus::fdo::Result<()> {
        self.shared
            .control
            .seek(offset_micros)
            .await
            .map_err(to_fdo)
    }
    /// Absolute seek in microseconds (used when the user clicks a lyric line).
    async fn set_position(&self, micros: i64) -> zbus::fdo::Result<()> {
        self.shared
            .control
            .set_position(micros)
            .await
            .map_err(to_fdo)
    }

    #[zbus(property)]
    async fn status(&self) -> String {
        self.shared
            .state
            .lock()
            .await
            .now_playing
            .status
            .as_str()
            .to_string()
    }
    #[zbus(property)]
    async fn title(&self) -> String {
        self.shared.state.lock().await.now_playing.title.clone()
    }
    #[zbus(property)]
    async fn artist(&self) -> String {
        self.shared.state.lock().await.now_playing.artist.clone()
    }
    #[zbus(property)]
    async fn album(&self) -> String {
        self.shared.state.lock().await.now_playing.album.clone()
    }
    #[zbus(property)]
    async fn art_url(&self) -> String {
        self.shared
            .state
            .lock()
            .await
            .now_playing
            .art_path
            .clone()
            .unwrap_or_default()
    }
    #[zbus(property)]
    async fn length_micros(&self) -> i64 {
        self.shared.state.lock().await.now_playing.length_micros
    }
    /// The live position, extrapolated at read time while playing.
    #[zbus(property)]
    async fn position_micros(&self) -> i64 {
        self.shared
            .state
            .lock()
            .await
            .now_playing
            .live_position_micros()
    }
    #[zbus(property)]
    async fn can_play(&self) -> bool {
        self.shared.state.lock().await.now_playing.caps.can_play
    }
    #[zbus(property)]
    async fn can_pause(&self) -> bool {
        self.shared.state.lock().await.now_playing.caps.can_pause
    }
    #[zbus(property)]
    async fn can_go_next(&self) -> bool {
        self.shared.state.lock().await.now_playing.caps.can_go_next
    }
    #[zbus(property)]
    async fn can_go_previous(&self) -> bool {
        self.shared
            .state
            .lock()
            .await
            .now_playing
            .caps
            .can_go_previous
    }
    #[zbus(property)]
    async fn can_seek(&self) -> bool {
        self.shared.state.lock().await.now_playing.caps.can_seek
    }
}

/// Emit `PropertiesChanged` for every Media property. Called on a now-playing
/// change; cheap and keeps the UI exactly in sync.
pub async fn emit_media(iface: &zbus::object_server::InterfaceRef<MediaIface>) {
    let m = iface.get().await;
    let e = iface.signal_emitter();
    let _ = m.status_changed(e).await;
    let _ = m.title_changed(e).await;
    let _ = m.artist_changed(e).await;
    let _ = m.album_changed(e).await;
    let _ = m.art_url_changed(e).await;
    let _ = m.length_micros_changed(e).await;
    let _ = m.position_micros_changed(e).await;
    let _ = m.can_play_changed(e).await;
    let _ = m.can_pause_changed(e).await;
    let _ = m.can_go_next_changed(e).await;
    let _ = m.can_go_previous_changed(e).await;
    let _ = m.can_seek_changed(e).await;
}

// --- Lyrics ----------------------------------------------------------------

pub struct LyricsIface {
    pub shared: Arc<Shared>,
}

#[zbus::interface(name = "ro.novabytecode.Mira.Lyrics")]
impl LyricsIface {
    /// All lyric lines as `(time_micros, text)` pairs (D-Bus `a(xs)`).
    async fn get_lines(&self) -> Vec<(i64, String)> {
        self.shared
            .state
            .lock()
            .await
            .lyrics
            .lines
            .iter()
            .map(|l| (l.time_micros, l.text.clone()))
            .collect()
    }

    /// Ask the active player to seek to the given line's timestamp.
    async fn seek_to_line(&self, index: u32) -> zbus::fdo::Result<()> {
        let micros = {
            let st = self.shared.state.lock().await;
            st.lyrics.lines.get(index as usize).map(|l| l.time_micros)
        };
        if let Some(micros) = micros {
            self.shared
                .control
                .set_position(micros)
                .await
                .map_err(to_fdo)?;
        }
        Ok(())
    }

    #[zbus(property)]
    async fn available(&self) -> bool {
        !self.shared.state.lock().await.lyrics.is_empty()
    }
    #[zbus(property)]
    async fn synced(&self) -> bool {
        self.shared.state.lock().await.lyrics.synced
    }
    #[zbus(property)]
    async fn source(&self) -> String {
        match self.shared.state.lock().await.lyrics.source {
            mira_core::lyrics::LyricsSource::Lrclib => "lrclib".to_string(),
            mira_core::lyrics::LyricsSource::None => "none".to_string(),
        }
    }

    /// Fresh lyrics were loaded for the current track.
    #[zbus(signal)]
    pub async fn lyrics_changed(emitter: &SignalEmitter<'_>) -> zbus::Result<()>;

    /// The highlighted line changed. `-1` means "before the first line".
    #[zbus(signal)]
    pub async fn active_line_changed(emitter: &SignalEmitter<'_>, index: i32) -> zbus::Result<()>;
}

pub async fn emit_lyrics_props(iface: &zbus::object_server::InterfaceRef<LyricsIface>) {
    let l = iface.get().await;
    let e = iface.signal_emitter();
    let _ = l.available_changed(e).await;
    let _ = l.synced_changed(e).await;
    let _ = l.source_changed(e).await;
}

// --- Power -----------------------------------------------------------------

pub struct PowerIface {
    pub shared: Arc<Shared>,
}

#[zbus::interface(name = "ro.novabytecode.Mira.Power")]
impl PowerIface {
    #[zbus(property)]
    async fn has_battery(&self) -> bool {
        self.shared.state.lock().await.power.has_battery
    }
    #[zbus(property)]
    async fn percentage(&self) -> f64 {
        self.shared.state.lock().await.power.percentage
    }
    #[zbus(property)]
    async fn state(&self) -> String {
        self.shared
            .state
            .lock()
            .await
            .power
            .state
            .as_str()
            .to_string()
    }
    #[zbus(property)]
    async fn on_ac(&self) -> bool {
        self.shared.state.lock().await.power.on_ac
    }

    #[zbus(signal)]
    pub async fn charger_connected(
        emitter: &SignalEmitter<'_>,
        percentage: f64,
    ) -> zbus::Result<()>;
    #[zbus(signal)]
    pub async fn charger_disconnected(
        emitter: &SignalEmitter<'_>,
        percentage: f64,
    ) -> zbus::Result<()>;
}

pub async fn emit_power(iface: &zbus::object_server::InterfaceRef<PowerIface>) {
    let p = iface.get().await;
    let e = iface.signal_emitter();
    let _ = p.has_battery_changed(e).await;
    let _ = p.percentage_changed(e).await;
    let _ = p.state_changed(e).await;
    let _ = p.on_ac_changed(e).await;
}

// --- System ----------------------------------------------------------------

pub struct SystemIface {
    pub shared: Arc<Shared>,
}

#[zbus::interface(name = "ro.novabytecode.Mira.System")]
impl SystemIface {
    /// Whether a fullscreen app is up. On Wayland the GNOME extension owns
    /// fullscreen detection, so this is advisory/diagnostic on Linux.
    #[zbus(property)]
    async fn fullscreen(&self) -> bool {
        self.shared.state.lock().await.fullscreen
    }
}

pub async fn emit_system(iface: &zbus::object_server::InterfaceRef<SystemIface>) {
    let s = iface.get().await;
    let e = iface.signal_emitter();
    let _ = s.fullscreen_changed(e).await;
}
