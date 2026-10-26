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

    use crate::dbus::{
        self, LyricsIface, MediaIface, PowerIface, ServiceIface, Shared, SystemIface, BUS_NAME,
        PATH,
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

        let shared = Arc::new(Shared {
            state: Mutex::new(AppState::new()),
            control: Arc::new(mpris_control),
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

        let query = TrackQuery {
            title: np.title.clone(),
            artist: np.artist.clone(),
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
}
