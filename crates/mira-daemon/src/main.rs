//! Mira daemon — the Linux host.
//!
//! Wires `mira-core` to the Linux data sources (MPRIS, UPower) and exposes the
//! whole state over D-Bus for the GNOME extension. Event-driven throughout: the
//! only timer is a low-rate active-lyric-line ticker that does nothing unless a
//! track is playing with synced lyrics.

#[cfg(not(target_os = "linux"))]
fn main() {
    eprintln!("mira-daemon only runs on Linux.");
}

#[cfg(target_os = "linux")]
mod dbus;

#[cfg(target_os = "linux")]
#[tokio::main(flavor = "current_thread")]
async fn main() {
    if let Err(e) = linux::run().await {
        tracing::error!("mira-daemon exited with error: {e}");
        std::process::exit(1);
    }
}

#[cfg(target_os = "linux")]
mod linux {
    use std::sync::Arc;
    use std::time::Duration;

    use tokio::sync::Mutex;

    use mira_core::app::AppState;
    use mira_core::event::{AppEvent, EventBus};
    use mira_core::lrclib::{LrclibClient, TrackQuery};
    use mira_core::lyrics::Lyrics;
    use mira_core::model::{NowPlaying, MICROS_PER_SEC};
    use mira_core::source::Source;
    use mira_core::store::{self, Config};

    use mira_platform_linux::{new_mpris, UPowerSource};

    use mira_core::pomodoro::PomodoroEngine;
    use mira_core::store::Store;

    use crate::dbus::{
        self, LyricsIface, MediaIface, PomodoroIface, PowerIface, RemindersIface, ServiceIface,
        Shared, ShelfIface, SystemIface, TodoIface, WaterIface, BUS_NAME, PATH,
    };

    pub async fn run() -> Result<(), Box<dyn std::error::Error>> {
        tracing_subscriber::fmt()
            .with_env_filter(
                tracing_subscriber::EnvFilter::try_from_default_env()
                    .unwrap_or_else(|_| "mira_daemon=info,warn".into()),
            )
            .init();

        let config = store::config_path()
            .and_then(|p| Config::load_or_default(&p))
            .unwrap_or_default();
        let locale = match config.language {
            store::Language::Ro => "ro",
            store::Language::En => "en",
        }
        .to_string();

        let bus = EventBus::default();

        // One session connection serves both the D-Bus server and MPRIS proxies.
        let conn = zbus::connection::Builder::session().build().await?;
        let (mpris_source, mpris_control) = new_mpris(conn.clone());

        // Local persistence (falls back to in-memory if the DB can't be opened).
        let store = store::db_path()
            .and_then(|p| Store::open(&p))
            .or_else(|e| {
                tracing::warn!("opening database failed ({e}); using in-memory store");
                Store::open_in_memory()
            })?;

        let shared = Arc::new(Shared {
            state: Mutex::new(AppState::new()),
            control: Arc::new(mpris_control),
            store: Mutex::new(store),
            pomodoro: Mutex::new(PomodoroEngine::new(config.pomodoro_config())),
            water_goal: config.water_goal,
        });

        // Register the interfaces, then claim the well-known name.
        let server = conn.object_server();
        server
            .at(
                PATH,
                ServiceIface {
                    version: env!("CARGO_PKG_VERSION").to_string(),
                    locale,
                },
            )
            .await?;
        server
            .at(
                PATH,
                MediaIface {
                    shared: shared.clone(),
                },
            )
            .await?;
        server
            .at(
                PATH,
                LyricsIface {
                    shared: shared.clone(),
                },
            )
            .await?;
        server
            .at(
                PATH,
                PowerIface {
                    shared: shared.clone(),
                },
            )
            .await?;
        server
            .at(
                PATH,
                SystemIface {
                    shared: shared.clone(),
                },
            )
            .await?;
        server
            .at(
                PATH,
                PomodoroIface {
                    shared: shared.clone(),
                },
            )
            .await?;
        server
            .at(
                PATH,
                WaterIface {
                    shared: shared.clone(),
                },
            )
            .await?;
        server
            .at(
                PATH,
                TodoIface {
                    shared: shared.clone(),
                },
            )
            .await?;
        server
            .at(
                PATH,
                ShelfIface {
                    shared: shared.clone(),
                },
            )
            .await?;
        server
            .at(
                PATH,
                RemindersIface {
                    shared: shared.clone(),
                },
            )
            .await?;
        conn.request_name(BUS_NAME).await?;
        tracing::info!("mira-daemon serving {BUS_NAME} at {PATH}");

        // Start the data sources.
        {
            let bus = bus.clone();
            let src = Box::new(mpris_source);
            tokio::spawn(async move {
                if let Err(e) = src.run(bus).await {
                    tracing::warn!("mpris source stopped: {e}");
                }
            });
        }
        {
            let bus = bus.clone();
            tokio::spawn(async move {
                if let Err(e) = Box::new(UPowerSource::new()).run(bus).await {
                    tracing::warn!("upower source stopped: {e}");
                }
            });
        }

        // Low-rate active-line ticker (no-op unless playing with synced lyrics).
        {
            let bus = bus.clone();
            let shared = shared.clone();
            tokio::spawn(async move {
                let mut tick = tokio::time::interval(Duration::from_millis(250));
                loop {
                    tick.tick().await;
                    let changed = shared.state.lock().await.tick_active_line();
                    if let Some(idx) = changed {
                        bus.publish(AppEvent::ActiveLine(idx));
                    }
                }
            });
        }

        // Pomodoro: tick once a second; emit property changes while running and
        // a phase-elapsed signal on rollover. Idle when the timer isn't running.
        {
            let conn = conn.clone();
            let shared = shared.clone();
            tokio::spawn(async move {
                let iface = match conn
                    .object_server()
                    .interface::<_, PomodoroIface>(PATH)
                    .await
                {
                    Ok(i) => i,
                    Err(_) => return,
                };
                let mut tick = tokio::time::interval(Duration::from_secs(1));
                loop {
                    tick.tick().await;
                    let result = shared.pomodoro.lock().await.tick();
                    match result {
                        mira_core::pomodoro::TickResult::Idle => {}
                        mira_core::pomodoro::TickResult::Counted => {
                            dbus::emit_pomodoro(&iface).await;
                        }
                        mira_core::pomodoro::TickResult::PhaseChanged(phase) => {
                            dbus::emit_pomodoro(&iface).await;
                            let _ = PomodoroIface::phase_elapsed(
                                iface.signal_emitter(),
                                dbus::phase_str(phase),
                            )
                            .await;
                        }
                    }
                }
            });
        }

        // Todo reminders: check periodically; fire once, then clear the reminder.
        {
            let conn = conn.clone();
            let shared = shared.clone();
            tokio::spawn(async move {
                let iface = match conn.object_server().interface::<_, TodoIface>(PATH).await {
                    Ok(i) => i,
                    Err(_) => return,
                };
                let mut tick = tokio::time::interval(Duration::from_secs(20));
                loop {
                    tick.tick().await;
                    let due = {
                        let store = shared.store.lock().await;
                        let due = store.due_reminders(dbus::now_epoch()).unwrap_or_default();
                        for t in &due {
                            let _ = store.set_todo_reminder(t.id, None);
                        }
                        due
                    };
                    if !due.is_empty() {
                        for t in &due {
                            let _ = TodoIface::reminder_due(iface.signal_emitter(), t.id, &t.text)
                                .await;
                        }
                        // The cleared reminders changed the list.
                        let _ = TodoIface::items_changed(iface.signal_emitter()).await;
                    }
                }
            });
        }

        // Separate reminders keep their due state until acknowledged. The flag
        // survives daemon restarts; signals are emitted once per due/snooze cycle.
        {
            let conn = conn.clone();
            let shared = shared.clone();
            tokio::spawn(async move {
                let Ok(iface) = conn
                    .object_server()
                    .interface::<_, RemindersIface>(PATH)
                    .await
                else {
                    return;
                };
                let mut tick = tokio::time::interval(Duration::from_secs(1));
                loop {
                    tick.tick().await;
                    let store = shared.store.lock().await;
                    match store.reminders() {
                        Ok(items) => {
                            for r in items
                                .iter()
                                .filter(|r| !r.notified && r.due_epoch <= dbus::now_epoch())
                            {
                                if RemindersIface::due(iface.signal_emitter(), r.id, &r.text)
                                    .await
                                    .is_ok()
                                {
                                    if let Err(e) = store.mark_reminder_notified(r.id) {
                                        tracing::warn!("marking reminder delivered: {e}");
                                    }
                                }
                            }
                        }
                        Err(e) => tracing::warn!("reading reminders: {e}"),
                    }
                }
            });
        }

        // Shut down cleanly on SIGTERM/SIGINT (systemd stop, Ctrl-C).
        let event_loop = event_loop(conn.clone(), bus.clone(), shared.clone());
        tokio::select! {
            _ = event_loop => {}
            _ = shutdown_signal() => tracing::info!("shutting down"),
        }
        Ok(())
    }

    async fn shutdown_signal() {
        use tokio::signal::unix::{signal, SignalKind};
        let mut term = signal(SignalKind::terminate()).expect("install SIGTERM handler");
        let mut int = signal(SignalKind::interrupt()).expect("install SIGINT handler");
        tokio::select! {
            _ = term.recv() => {}
            _ = int.recv() => {}
        }
    }

    async fn event_loop(conn: zbus::Connection, bus: EventBus, shared: Arc<Shared>) {
        let server = conn.object_server();
        let media = server.interface::<_, MediaIface>(PATH).await.ok();
        let lyrics = server.interface::<_, LyricsIface>(PATH).await.ok();
        let power = server.interface::<_, PowerIface>(PATH).await.ok();
        let system = server.interface::<_, SystemIface>(PATH).await.ok();

        let lrclib = LrclibClient::new().ok();
        let mut last_lyrics_key: Option<(String, String)> = None;

        let mut rx = bus.subscribe();
        loop {
            let ev = match rx.recv().await {
                Ok(ev) => ev,
                Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                Err(_) => break,
            };

            // Fold the event into shared state first.
            shared.state.lock().await.apply(&ev);

            match &ev {
                AppEvent::NowPlaying(np) => {
                    if let Some(m) = &media {
                        dbus::emit_media(m).await;
                    }
                    maybe_fetch_lyrics(np, &bus, &lrclib, &mut last_lyrics_key);
                }
                AppEvent::Lyrics(_) => {
                    if let Some(l) = &lyrics {
                        dbus::emit_lyrics_props(l).await;
                        let _ = LyricsIface::lyrics_changed(l.signal_emitter()).await;
                        // A fresh set implies a (possibly new) active line.
                        let idx = shared.state.lock().await.active_line;
                        let _ = LyricsIface::active_line_changed(
                            l.signal_emitter(),
                            idx.map(|i| i as i32).unwrap_or(-1),
                        )
                        .await;
                    }
                }
                AppEvent::ActiveLine(idx) => {
                    if let Some(l) = &lyrics {
                        let _ = LyricsIface::active_line_changed(
                            l.signal_emitter(),
                            idx.map(|i| i as i32).unwrap_or(-1),
                        )
                        .await;
                    }
                }
                AppEvent::Power(_) => {
                    if let Some(p) = &power {
                        dbus::emit_power(p).await;
                    }
                }
                AppEvent::System(se) => {
                    if let Some(p) = &power {
                        use mira_core::model::SystemEvent::*;
                        match se {
                            ChargerConnected { percentage } => {
                                let _ =
                                    PowerIface::charger_connected(p.signal_emitter(), *percentage)
                                        .await;
                            }
                            ChargerDisconnected { percentage } => {
                                let _ = PowerIface::charger_disconnected(
                                    p.signal_emitter(),
                                    *percentage,
                                )
                                .await;
                            }
                            _ => {} // phase-2 events
                        }
                    }
                }
                AppEvent::Fullscreen(_) => {
                    if let Some(s) = &system {
                        dbus::emit_system(s).await;
                    }
                }
            }
        }
    }

    /// Kick off an LRCLIB fetch when the track changes. Results come back as an
    /// `AppEvent::Lyrics` on the bus, keeping the event loop non-blocking.
    fn maybe_fetch_lyrics(
        np: &NowPlaying,
        bus: &EventBus,
        lrclib: &Option<LrclibClient>,
        last_key: &mut Option<(String, String)>,
    ) {
        let Some(client) = lrclib.clone() else {
            return;
        };
        if !np.has_track() {
            return;
        }
        let key = (np.title.clone(), np.artist.clone());
        if last_key.as_ref() == Some(&key) {
            return;
        }
        *last_key = Some(key);

        // Browser/YouTube tracks arrive as a messy single title ("Artist - Song
        // (Official Video)") with no artist. Clean it up so LRCLIB can match.
        let (title, artist) = clean_track(&np.title, &np.artist);
        let query = TrackQuery {
            title,
            artist,
            album: np.album.clone(),
            duration_secs: (np.length_micros > 0).then_some(np.length_micros / MICROS_PER_SEC),
        };
        let bus = bus.clone();
        tokio::spawn(async move {
            match client.fetch(&query).await {
                Ok(lyrics) => bus.publish(AppEvent::Lyrics(lyrics)),
                Err(_) => bus.publish(AppEvent::Lyrics(Lyrics::default())),
            }
        });
    }

    /// Normalise a now-playing title/artist for lyric lookup. Strips common
    /// video-title noise and, when the artist is empty, splits an "Artist - Song"
    /// title (the usual shape for YouTube played through a browser).
    fn clean_track(raw_title: &str, raw_artist: &str) -> (String, String) {
        let mut title = strip_noise(raw_title);
        let mut artist = strip_noise(raw_artist);
        // Drop a channel "- Topic" suffix that YouTube Music adds to the artist.
        if let Some(stripped) = artist.strip_suffix("- Topic") {
            artist = stripped.trim().to_string();
        }
        if artist.is_empty() {
            if let Some((a, t)) = title.split_once(" - ") {
                artist = a.trim().to_string();
                title = t.trim().to_string();
            }
        }
        (title, artist)
    }

    /// Remove bracketed/segment noise such as "(Official Video)", "[Lyrics]",
    /// "(feat. X)", "| NPR", trailing "- YouTube", and collapse whitespace.
    fn strip_noise(s: &str) -> String {
        let mut out = String::with_capacity(s.len());
        let mut depth_paren = 0i32;
        let mut depth_brack = 0i32;
        for ch in s.chars() {
            match ch {
                '(' => depth_paren += 1,
                ')' => depth_paren = (depth_paren - 1).max(0),
                '[' => depth_brack += 1,
                ']' => depth_brack = (depth_brack - 1).max(0),
                _ if depth_paren == 0 && depth_brack == 0 => out.push(ch),
                _ => {}
            }
        }
        // Cut anything after a pipe (e.g. "Song | Live at ...").
        if let Some(idx) = out.find('|') {
            out.truncate(idx);
        }
        let out = out
            .trim()
            .trim_end_matches("- YouTube")
            .trim_end_matches("- Topic")
            .trim();
        out.split_whitespace().collect::<Vec<_>>().join(" ")
    }
}
