//! The D-Bus surface: `ro.novabytecode.Mira.{Service,Media,Lyrics,Power,System}`
//! served at `/ro/novabytecode/Mira` on the session bus.
//!
//! Every interface is a thin view over the shared [`AppState`]; property
//! getters read it, methods drive the active player, and the daemon's event
//! loop (see `main.rs`) mutates the state and fires the `*_changed` /signal
//! notifications. UIs (the GNOME extension) hold no business logic — they read
//! properties and react to signals.

use std::sync::{
    atomic::{AtomicU64, Ordering},
    Arc,
};
use std::time::{SystemTime, UNIX_EPOCH};

use tokio::sync::Mutex;
use zbus::object_server::SignalEmitter;

use mira_core::app::AppState;
use mira_core::model::PomodoroPhase;
use mira_core::pomodoro::PomodoroEngine;
use mira_core::source::MediaControl;
use mira_core::store::Store;

pub const PATH: &str = "/ro/novabytecode/Mira";
pub const BUS_NAME: &str = "ro.novabytecode.Mira";

/// State shared by all interfaces and the event loop.
pub struct Shared {
    pub state: Mutex<AppState>,
    pub control: Arc<dyn MediaControl>,
    pub store: Mutex<Store>,
    pub pomodoro: Mutex<PomodoroEngine>,
    pub water_goal: u32,
    pub bus: mira_core::event::EventBus,
    pub lyrics_generation: AtomicU64,
    pub lyrics_manual: Mutex<String>,
}

impl Shared {
    /// Apply lyric results while holding the same request lock used to start a
    /// lookup. Superseded responses/resets cannot race a new manual request.
    pub async fn apply_event(&self, event: &mira_core::event::AppEvent) -> bool {
        if let mira_core::event::AppEvent::LyricsResult { request_id, .. } = event {
            let _request = self.lyrics_manual.lock().await;
            if *request_id != self.lyrics_generation.load(Ordering::SeqCst) {
                return false;
            }
            self.state.lock().await.apply(event);
        } else {
            self.state.lock().await.apply(event);
        }
        true
    }

    /// Serialize request identity, label and reset as one operation. A delayed
    /// reset from an older request must never erase newer/manual lyrics.
    pub async fn begin_lyrics(&self, label: String) -> u64 {
        let mut manual = self.lyrics_manual.lock().await;
        let request_id = self.lyrics_generation.fetch_add(1, Ordering::SeqCst) + 1;
        *manual = label;
        self.bus.publish(mira_core::event::AppEvent::LyricsResult {
            request_id,
            lyrics: mira_core::lyrics::Lyrics::default(),
        });
        request_id
    }
}

fn to_fdo(e: mira_core::Error) -> zbus::fdo::Error {
    zbus::fdo::Error::Failed(e.to_string())
}

/// Seconds since the Unix epoch.
pub fn now_epoch() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// A per-day key for the water log. Uses the UTC day number, so the counter
/// resets at UTC midnight (good enough for phase 1 of the feature).
pub fn today_key() -> String {
    mira_core::store::local_day(now_epoch())
}

pub fn phase_str(p: PomodoroPhase) -> &'static str {
    match p {
        PomodoroPhase::Work => "work",
        PomodoroPhase::ShortBreak => "short_break",
        PomodoroPhase::LongBreak => "long_break",
    }
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

    /// Explicit user lookup for private/incorrect browser metadata. Only the
    /// supplied title/artist are sent to LRCLIB; browser privacy stays intact.
    async fn lookup(&self, title: String, artist: String) -> zbus::fdo::Result<bool> {
        let title = title.trim();
        let artist = artist.trim();
        if title.is_empty() || title.chars().count() > 200 || artist.chars().count() > 200 {
            return Err(zbus::fdo::Error::InvalidArgs(
                "Enter a title (1–200 characters) and optional artist".into(),
            ));
        }
        let label = if artist.is_empty() {
            title.into()
        } else {
            format!("{artist} — {title}")
        };
        let request_id = self.shared.begin_lyrics(label).await;
        let query = mira_core::lrclib::TrackQuery::new(title, artist);
        let lyrics = mira_core::lrclib::LrclibClient::new()
            .map_err(to_fdo)?
            .fetch(&query)
            .await
            .unwrap_or_default();
        if request_id != self.shared.lyrics_generation.load(Ordering::SeqCst) {
            return Err(zbus::fdo::Error::Failed(
                "Playback changed during lookup; search again".into(),
            ));
        }
        let found = !lyrics.is_empty();
        self.shared
            .bus
            .publish(mira_core::event::AppEvent::LyricsResult { request_id, lyrics });
        Ok(found)
    }

    #[zbus(property)]
    async fn manual_query(&self) -> String {
        self.shared.lyrics_manual.lock().await.clone()
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
    let _ = l.manual_query_changed(e).await;
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

// --- Pomodoro (focus timer) ------------------------------------------------

pub struct PomodoroIface {
    pub shared: Arc<Shared>,
}

#[zbus::interface(name = "ro.novabytecode.Mira.Pomodoro")]
impl PomodoroIface {
    #[zbus(property)]
    async fn phase(&self) -> String {
        phase_str(self.shared.pomodoro.lock().await.snapshot().phase).to_string()
    }
    #[zbus(property)]
    async fn remaining_secs(&self) -> i64 {
        self.shared.pomodoro.lock().await.snapshot().remaining_secs
    }
    #[zbus(property)]
    async fn running(&self) -> bool {
        self.shared.pomodoro.lock().await.is_running()
    }
    #[zbus(property)]
    async fn completed_cycles(&self) -> u32 {
        self.shared
            .pomodoro
            .lock()
            .await
            .snapshot()
            .completed_cycles
    }

    async fn start(&self, #[zbus(signal_emitter)] e: SignalEmitter<'_>) -> zbus::fdo::Result<()> {
        self.shared.pomodoro.lock().await.start();
        let _ = self.running_changed(&e).await;
        Ok(())
    }
    async fn pause(&self, #[zbus(signal_emitter)] e: SignalEmitter<'_>) -> zbus::fdo::Result<()> {
        self.shared.pomodoro.lock().await.pause();
        let _ = self.running_changed(&e).await;
        Ok(())
    }
    async fn toggle(&self, #[zbus(signal_emitter)] e: SignalEmitter<'_>) -> zbus::fdo::Result<()> {
        self.shared.pomodoro.lock().await.toggle();
        let _ = self.running_changed(&e).await;
        Ok(())
    }
    async fn reset(&self, #[zbus(signal_emitter)] e: SignalEmitter<'_>) -> zbus::fdo::Result<()> {
        self.shared.pomodoro.lock().await.reset();
        pomo_emit_all(self, &e).await;
        Ok(())
    }
    async fn skip(&self, #[zbus(signal_emitter)] e: SignalEmitter<'_>) -> zbus::fdo::Result<()> {
        let phase = self.shared.pomodoro.lock().await.skip();
        pomo_emit_all(self, &e).await;
        let _ = Self::phase_elapsed(&e, phase_str(phase)).await;
        Ok(())
    }

    /// The timer rolled over into a new phase (for a notch alert).
    #[zbus(signal)]
    pub async fn phase_elapsed(emitter: &SignalEmitter<'_>, phase: &str) -> zbus::Result<()>;
}

async fn pomo_emit_all(iface: &PomodoroIface, e: &SignalEmitter<'_>) {
    let _ = iface.phase_changed(e).await;
    let _ = iface.remaining_secs_changed(e).await;
    let _ = iface.running_changed(e).await;
    let _ = iface.completed_cycles_changed(e).await;
}

pub async fn emit_pomodoro(iface: &zbus::object_server::InterfaceRef<PomodoroIface>) {
    let p = iface.get().await;
    pomo_emit_all(&p, iface.signal_emitter()).await;
}

// --- Water -----------------------------------------------------------------

pub struct WaterIface {
    pub shared: Arc<Shared>,
}

#[zbus::interface(name = "ro.novabytecode.Mira.Water")]
impl WaterIface {
    #[zbus(property)]
    async fn glasses_today(&self) -> u32 {
        self.shared
            .store
            .lock()
            .await
            .water_glasses(&today_key())
            .unwrap_or(0)
    }
    #[zbus(property)]
    async fn goal(&self) -> u32 {
        self.shared.water_goal
    }

    async fn add(&self, #[zbus(signal_emitter)] e: SignalEmitter<'_>) -> zbus::fdo::Result<()> {
        self.shared
            .store
            .lock()
            .await
            .water_adjust(&today_key(), 1)
            .map_err(to_fdo)?;
        let _ = self.glasses_today_changed(&e).await;
        Ok(())
    }
    async fn remove(&self, #[zbus(signal_emitter)] e: SignalEmitter<'_>) -> zbus::fdo::Result<()> {
        self.shared
            .store
            .lock()
            .await
            .water_adjust(&today_key(), -1)
            .map_err(to_fdo)?;
        let _ = self.glasses_today_changed(&e).await;
        Ok(())
    }
}

// --- Todo ------------------------------------------------------------------

pub struct TodoIface {
    pub shared: Arc<Shared>,
}

#[zbus::interface(name = "ro.novabytecode.Mira.Todo")]
impl TodoIface {
    /// All todos as `(id, text, done, remind_at_epoch)`; remind 0 means none.
    async fn get_items(&self) -> Vec<(i64, String, bool, i64)> {
        self.shared
            .store
            .lock()
            .await
            .todos()
            .unwrap_or_default()
            .into_iter()
            .map(|t| (t.id, t.text, t.done, t.remind_at_epoch.unwrap_or(0)))
            .collect()
    }

    async fn add(
        &self,
        text: String,
        #[zbus(signal_emitter)] e: SignalEmitter<'_>,
    ) -> zbus::fdo::Result<i64> {
        let id = self
            .shared
            .store
            .lock()
            .await
            .add_todo(&text)
            .map_err(to_fdo)?;
        let _ = Self::items_changed(&e).await;
        Ok(id)
    }

    async fn toggle(
        &self,
        id: i64,
        #[zbus(signal_emitter)] e: SignalEmitter<'_>,
    ) -> zbus::fdo::Result<()> {
        {
            let store = self.shared.store.lock().await;
            let todos = store.todos().map_err(to_fdo)?;
            if let Some(t) = todos.iter().find(|t| t.id == id) {
                store.set_todo_done(id, !t.done).map_err(to_fdo)?;
            }
        }
        let _ = Self::items_changed(&e).await;
        Ok(())
    }

    async fn remove(
        &self,
        id: i64,
        #[zbus(signal_emitter)] e: SignalEmitter<'_>,
    ) -> zbus::fdo::Result<()> {
        self.shared
            .store
            .lock()
            .await
            .remove_todo(id)
            .map_err(to_fdo)?;
        let _ = Self::items_changed(&e).await;
        Ok(())
    }

    /// Set a reminder time (epoch seconds); `0` clears it.
    async fn set_reminder(
        &self,
        id: i64,
        epoch: i64,
        #[zbus(signal_emitter)] e: SignalEmitter<'_>,
    ) -> zbus::fdo::Result<()> {
        let r = if epoch > 0 { Some(epoch) } else { None };
        self.shared
            .store
            .lock()
            .await
            .set_todo_reminder(id, r)
            .map_err(to_fdo)?;
        let _ = Self::items_changed(&e).await;
        Ok(())
    }

    #[zbus(signal)]
    pub async fn items_changed(emitter: &SignalEmitter<'_>) -> zbus::Result<()>;

    /// A todo's reminder came due (for a notch alert).
    #[zbus(signal)]
    pub async fn reminder_due(emitter: &SignalEmitter<'_>, id: i64, text: &str)
        -> zbus::Result<()>;
}

// --- Standalone reminders --------------------------------------------------

pub struct RemindersIface {
    pub shared: Arc<Shared>,
}

#[zbus::interface(name = "ro.novabytecode.Mira.Reminders")]
impl RemindersIface {
    async fn get_items(&self) -> zbus::fdo::Result<Vec<(i64, String, i64, i64, bool)>> {
        Ok(self
            .shared
            .store
            .lock()
            .await
            .reminders()
            .map_err(to_fdo)?
            .into_iter()
            .map(|r| (r.id, r.text, r.due_epoch, r.created_epoch, r.notified))
            .collect())
    }
    async fn add(
        &self,
        text: String,
        due_epoch: i64,
        #[zbus(signal_emitter)] e: SignalEmitter<'_>,
    ) -> zbus::fdo::Result<i64> {
        let id = self
            .shared
            .store
            .lock()
            .await
            .add_reminder(&text, due_epoch, now_epoch())
            .map_err(to_fdo)?;
        Self::items_changed(&e).await.ok();
        Ok(id)
    }
    async fn preview_natural(&self, input: String) -> zbus::fdo::Result<String> {
        Ok(match mira_core::natural::parse_local(&input,now_epoch()) {
            Ok(draft)=>serde_json::json!({"valid":true,"recognized":true,"draft":draft}),
            Err(error)=>serde_json::json!({"valid":false,"recognized":mira_core::natural::looks_natural(&input),"error":error.to_string()}),
        }.to_string())
    }
    async fn add_natural(
        &self,
        input: String,
        #[zbus(signal_emitter)] e: SignalEmitter<'_>,
    ) -> zbus::fdo::Result<i64> {
        let id = self
            .shared
            .store
            .lock()
            .await
            .add_natural_reminder(&input, now_epoch())
            .map_err(to_fdo)?;
        Self::items_changed(&e).await.ok();
        Ok(id)
    }
    async fn get_details(&self) -> zbus::fdo::Result<String> {
        serde_json::to_string(&self.shared.store.lock().await.reminders().map_err(to_fdo)?)
            .map_err(|e| zbus::fdo::Error::Failed(e.to_string()))
    }
    async fn complete_occurrence(
        &self,
        id: i64,
        expected_due: i64,
        #[zbus(signal_emitter)] e: SignalEmitter<'_>,
    ) -> zbus::fdo::Result<()> {
        self.shared
            .store
            .lock()
            .await
            .complete_occurrence(id, expected_due, now_epoch())
            .map_err(to_fdo)?;
        Self::items_changed(&e).await.ok();
        Ok(())
    }
    async fn complete(
        &self,
        id: i64,
        #[zbus(signal_emitter)] e: SignalEmitter<'_>,
    ) -> zbus::fdo::Result<()> {
        self.shared
            .store
            .lock()
            .await
            .complete_reminder(id, now_epoch())
            .map_err(to_fdo)?;
        Self::items_changed(&e).await.ok();
        Ok(())
    }
    async fn remove(
        &self,
        id: i64,
        #[zbus(signal_emitter)] e: SignalEmitter<'_>,
    ) -> zbus::fdo::Result<()> {
        self.shared
            .store
            .lock()
            .await
            .remove_reminder(id)
            .map_err(to_fdo)?;
        Self::items_changed(&e).await.ok();
        Ok(())
    }
    async fn snooze(
        &self,
        id: i64,
        due_epoch: i64,
        #[zbus(signal_emitter)] e: SignalEmitter<'_>,
    ) -> zbus::fdo::Result<()> {
        self.shared
            .store
            .lock()
            .await
            .snooze_reminder(id, due_epoch, now_epoch())
            .map_err(to_fdo)?;
        Self::items_changed(&e).await.ok();
        Ok(())
    }
    #[zbus(signal)]
    pub async fn items_changed(emitter: &SignalEmitter<'_>) -> zbus::Result<()>;
    #[zbus(signal)]
    pub async fn due(emitter: &SignalEmitter<'_>, id: i64, text: &str) -> zbus::Result<()>;
}

// --- Daily summary ---------------------------------------------------------
pub struct SummaryIface {
    pub shared: Arc<Shared>,
}
#[zbus::interface(name = "ro.novabytecode.Mira.Summary")]
impl SummaryIface {
    async fn get_today(&self) -> zbus::fdo::Result<String> {
        let summary = self
            .shared
            .store
            .lock()
            .await
            .daily_summary(&today_key())
            .map_err(to_fdo)?;
        serde_json::to_string(&summary).map_err(|e| zbus::fdo::Error::Failed(e.to_string()))
    }
    async fn get_day(&self, day: String) -> zbus::fdo::Result<String> {
        let summary = self
            .shared
            .store
            .lock()
            .await
            .daily_summary(&day)
            .map_err(to_fdo)?;
        serde_json::to_string(&summary).map_err(|e| zbus::fdo::Error::Failed(e.to_string()))
    }
}

// --- Shelf -----------------------------------------------------------------

pub struct ShelfIface {
    pub shared: Arc<Shared>,
}

#[zbus::interface(name = "ro.novabytecode.Mira.Shelf")]
impl ShelfIface {
    /// Shelf items as `(id, path, added_epoch)`. Paths only — never contents.
    async fn get_items(&self) -> Vec<(i64, String, i64)> {
        self.shared
            .store
            .lock()
            .await
            .shelf_items()
            .unwrap_or_default()
            .into_iter()
            .map(|s| (s.id, s.path, s.added_epoch))
            .collect()
    }

    async fn add(
        &self,
        path: String,
        #[zbus(signal_emitter)] e: SignalEmitter<'_>,
    ) -> zbus::fdo::Result<i64> {
        let id = self
            .shared
            .store
            .lock()
            .await
            .add_shelf_item(&path, now_epoch())
            .map_err(to_fdo)?;
        let _ = Self::items_changed(&e).await;
        Ok(id)
    }

    async fn remove(
        &self,
        id: i64,
        #[zbus(signal_emitter)] e: SignalEmitter<'_>,
    ) -> zbus::fdo::Result<()> {
        self.shared
            .store
            .lock()
            .await
            .remove_shelf_item(id)
            .map_err(to_fdo)?;
        let _ = Self::items_changed(&e).await;
        Ok(())
    }

    #[zbus(signal)]
    pub async fn items_changed(emitter: &SignalEmitter<'_>) -> zbus::Result<()>;
}

#[cfg(test)]
mod lyrics_request_tests {
    use super::*;
    use mira_core::{
        event::{AppEvent, EventBus},
        lyrics::{Lyrics, LyricsSource},
        pomodoro::PomodoroConfig,
    };
    struct NoopControl;
    #[async_trait::async_trait]
    impl MediaControl for NoopControl {
        async fn play_pause(&self) -> mira_core::Result<()> {
            Ok(())
        }
        async fn next(&self) -> mira_core::Result<()> {
            Ok(())
        }
        async fn previous(&self) -> mira_core::Result<()> {
            Ok(())
        }
        async fn seek(&self, _: i64) -> mira_core::Result<()> {
            Ok(())
        }
        async fn set_position(&self, _: i64) -> mira_core::Result<()> {
            Ok(())
        }
    }
    #[tokio::test]
    async fn late_result_and_reset_cannot_overwrite_a_new_manual_lookup() {
        let shared = Shared {
            state: Mutex::new(AppState::new()),
            control: Arc::new(NoopControl),
            store: Mutex::new(Store::open_in_memory().unwrap()),
            pomodoro: Mutex::new(PomodoroEngine::new(PomodoroConfig::default())),
            water_goal: 8,
            bus: EventBus::default(),
            lyrics_generation: AtomicU64::new(0),
            lyrics_manual: Mutex::new(String::new()),
        };
        let old = shared.begin_lyrics(String::new()).await;
        let newest = shared.begin_lyrics("Artist — Manual song".into()).await;
        assert!(
            shared
                .apply_event(&AppEvent::LyricsResult {
                    request_id: newest,
                    lyrics: Lyrics::from_plain("Correct manual lyrics", LyricsSource::Lrclib)
                })
                .await
        );
        assert!(
            !shared
                .apply_event(&AppEvent::LyricsResult {
                    request_id: old,
                    lyrics: Lyrics::default()
                })
                .await
        );
        assert!(
            !shared
                .apply_event(&AppEvent::LyricsResult {
                    request_id: old,
                    lyrics: Lyrics::from_plain("Stale song", LyricsSource::Lrclib)
                })
                .await
        );
        assert_eq!(
            shared.state.lock().await.lyrics.lines[0].text,
            "Correct manual lyrics"
        );
        assert_eq!(
            shared.lyrics_manual.lock().await.as_str(),
            "Artist — Manual song"
        );
        let auto = shared.begin_lyrics(String::new()).await;
        assert!(
            shared
                .apply_event(&AppEvent::LyricsResult {
                    request_id: auto,
                    lyrics: Lyrics::default()
                })
                .await
        );
        assert!(shared.state.lock().await.lyrics.is_empty());
    }
}
