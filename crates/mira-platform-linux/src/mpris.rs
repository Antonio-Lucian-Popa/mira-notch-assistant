//! MPRIS media source and control.
//!
//! MPRIS players each own a bus name `org.mpris.MediaPlayer2.<player>` on the
//! session bus and expose `org.mpris.MediaPlayer2.Player` at
//! `/org/mpris/MediaPlayer2`. We pick an active player, publish its state, and
//! react to `PropertiesChanged` / `Seeked` / `NameOwnerChanged` — never poll.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;

use async_trait::async_trait;
use futures_util::StreamExt;
use tokio::sync::Mutex;
use zbus::Connection;
use zbus::OwnedVariant as OwnedValue;

use mira_core::error::{Error, Result};
use mira_core::event::{AppEvent, EventBus};
use mira_core::model::{MediaCapabilities, NowPlaying, PlaybackStatus};
use mira_core::source::{MediaControl, MediaSource, Source};

const MPRIS_PREFIX: &str = "org.mpris.MediaPlayer2.";

#[zbus::proxy(
    interface = "org.mpris.MediaPlayer2.Player",
    default_path = "/org/mpris/MediaPlayer2"
)]
trait Player {
    fn play_pause(&self) -> zbus::Result<()>;
    fn next(&self) -> zbus::Result<()>;
    fn previous(&self) -> zbus::Result<()>;
    fn seek(&self, offset: i64) -> zbus::Result<()>;
    fn set_position(&self, track_id: &zbus::ObjectPath<'_>, position: i64) -> zbus::Result<()>;

    #[zbus(property)]
    fn playback_status(&self) -> zbus::Result<String>;
    #[zbus(property)]
    fn metadata(&self) -> zbus::Result<HashMap<String, OwnedValue>>;
    #[zbus(property)]
    fn position(&self) -> zbus::Result<i64>;
    #[zbus(property)]
    fn can_play(&self) -> zbus::Result<bool>;
    #[zbus(property)]
    fn can_pause(&self) -> zbus::Result<bool>;
    #[zbus(property)]
    fn can_go_next(&self) -> zbus::Result<bool>;
    #[zbus(property)]
    fn can_go_previous(&self) -> zbus::Result<bool>;
    #[zbus(property)]
    fn can_seek(&self) -> zbus::Result<bool>;

    #[zbus(signal)]
    fn seeked(&self, position: i64) -> zbus::Result<()>;
}

/// Shared handle to the currently selected player's bus name.
type CurrentPlayer = Arc<Mutex<Option<String>>>;

/// Media source. Owns the session connection and discovers the active player.
pub struct MprisSource {
    conn: Connection,
    current: CurrentPlayer,
}

/// Control surface that targets whatever player the source currently tracks.
#[derive(Clone)]
pub struct MprisControl {
    conn: Connection,
    current: CurrentPlayer,
}

/// Build a paired media source + control sharing one session connection.
pub fn new_mpris(conn: Connection) -> (MprisSource, MprisControl) {
    let current: CurrentPlayer = Arc::new(Mutex::new(None));
    (
        MprisSource {
            conn: conn.clone(),
            current: current.clone(),
        },
        MprisControl { conn, current },
    )
}

impl MediaSource for MprisSource {}

#[async_trait]
impl Source for MprisSource {
    fn name(&self) -> &'static str {
        "mpris"
    }

    async fn run(self: Box<Self>, bus: EventBus) -> Result<()> {
        let dbus = zbus::fdo::DBusProxy::new(&self.conn)
            .await
            .map_err(zbus_err)?;

        loop {
            let name = match pick_player(&self.conn).await {
                Some(n) => n,
                None => {
                    // No player: show "nothing playing" and wait for one.
                    *self.current.lock().await = None;
                    bus.publish(AppEvent::NowPlaying(NowPlaying::default()));
                    wait_for_mpris_name(&dbus).await?;
                    continue;
                }
            };
            *self.current.lock().await = Some(name.clone());

            let proxy = build_player(&self.conn, &name).await?;
            bus.publish(AppEvent::NowPlaying(read_now_playing(&proxy, &name).await));

            // Subscribe to everything that can change this player's state.
            let mut status = proxy.receive_playback_status_changed().await;
            let mut metadata = proxy.receive_metadata_changed().await;
            let mut position = proxy.receive_position_changed().await;
            let mut seeked = proxy.receive_seeked().await.map_err(zbus_err)?;
            let mut owners = dbus.receive_name_owner_changed().await.map_err(zbus_err)?;

            loop {
                tokio::select! {
                    Some(_) = status.next() => republish(&proxy, &name, &bus).await,
                    Some(_) = metadata.next() => republish(&proxy, &name, &bus).await,
                    Some(_) = position.next() => republish(&proxy, &name, &bus).await,
                    Some(_) = seeked.next() => republish(&proxy, &name, &bus).await,
                    Some(sig) = owners.next() => {
                        if let Ok(args) = sig.args() {
                            let changed = args.name().as_str().to_string();
                            let has_new = !args.new_owner().is_none();
                            // Our player vanished, or a different player showed
                            // up — re-evaluate who is active.
                            if (changed == name && !has_new)
                                || (changed.starts_with(MPRIS_PREFIX) && has_new && changed != name)
                            {
                                break;
                            }
                        }
                    }
                    else => break,
                }
            }
        }
    }
}

#[async_trait]
impl MediaControl for MprisControl {
    async fn play_pause(&self) -> Result<()> {
        if let Some(p) = self.player().await? {
            p.play_pause().await.map_err(zbus_err)?;
        }
        Ok(())
    }

    async fn next(&self) -> Result<()> {
        if let Some(p) = self.player().await? {
            p.next().await.map_err(zbus_err)?;
        }
        Ok(())
    }

    async fn previous(&self) -> Result<()> {
        if let Some(p) = self.player().await? {
            p.previous().await.map_err(zbus_err)?;
        }
        Ok(())
    }

    async fn seek(&self, offset_micros: i64) -> Result<()> {
        if let Some(p) = self.player().await? {
            p.seek(offset_micros).await.map_err(zbus_err)?;
        }
        Ok(())
    }

    async fn set_position(&self, micros: i64) -> Result<()> {
        let Some(p) = self.player().await? else {
            return Ok(());
        };
        let md = p.metadata().await.unwrap_or_default();
        if let Some(track_id) = md
            .get("mpris:trackid")
            .and_then(|v| zbus::OwnedObjectPath::try_from(v.clone()).ok())
        {
            let path: zbus::ObjectPath = track_id.as_ref();
            p.set_position(&path, micros).await.map_err(zbus_err)?;
        } else {
            // Player did not expose a track id: approximate with a relative seek.
            let cur = p.position().await.unwrap_or(0);
            p.seek(micros - cur).await.map_err(zbus_err)?;
        }
        Ok(())
    }
}

impl MprisControl {
    async fn player(&self) -> Result<Option<PlayerProxy<'static>>> {
        let name = self.current.lock().await.clone();
        match name {
            Some(n) => Ok(Some(build_player(&self.conn, &n).await?)),
            None => Ok(None),
        }
    }
}

async fn build_player(conn: &Connection, name: &str) -> Result<PlayerProxy<'static>> {
    PlayerProxy::builder(conn)
        .destination(name.to_string())
        .build()
        .await
        .map_err(zbus_err)
}

async fn republish(proxy: &PlayerProxy<'_>, name: &str, bus: &EventBus) {
    bus.publish(AppEvent::NowPlaying(read_now_playing(proxy, name).await));
}

async fn read_now_playing(proxy: &PlayerProxy<'_>, player_id: &str) -> NowPlaying {
    let status = proxy
        .playback_status()
        .await
        .map(|s| PlaybackStatus::from_mpris(&s))
        .unwrap_or_default();
    let md = proxy.metadata().await.unwrap_or_default();

    NowPlaying {
        status,
        title: md_string(&md, "xesam:title").unwrap_or_default(),
        artist: md_string_list(&md, "xesam:artist")
            .map(|v| v.join(", "))
            .or_else(|| md_string(&md, "xesam:artist"))
            .unwrap_or_default(),
        album: md_string(&md, "xesam:album").unwrap_or_default(),
        art_path: md_string(&md, "mpris:artUrl"),
        length_micros: md_i64(&md, "mpris:length").unwrap_or(0),
        position_micros: proxy.position().await.unwrap_or(0),
        caps: MediaCapabilities {
            can_play: proxy.can_play().await.unwrap_or(false),
            can_pause: proxy.can_pause().await.unwrap_or(false),
            can_go_next: proxy.can_go_next().await.unwrap_or(false),
            can_go_previous: proxy.can_go_previous().await.unwrap_or(false),
            can_seek: proxy.can_seek().await.unwrap_or(false),
        },
        player_id: player_id.to_string(),
        sampled_at: Some(Instant::now()),
    }
}

/// Pick the active MPRIS player: a playing one if any, else the first found.
async fn pick_player(conn: &Connection) -> Option<String> {
    let dbus = zbus::fdo::DBusProxy::new(conn).await.ok()?;
    let names = dbus.list_names().await.ok()?;
    let mpris: Vec<String> = names
        .into_iter()
        .map(|n| n.as_str().to_string())
        .filter(|n| n.starts_with(MPRIS_PREFIX))
        .collect();

    for n in &mpris {
        if let Ok(p) = build_player(conn, n).await {
            if p.playback_status().await.as_deref() == Ok("Playing") {
                return Some(n.clone());
            }
        }
    }
    mpris.into_iter().next()
}

/// Block until an MPRIS player appears on the bus.
async fn wait_for_mpris_name(dbus: &zbus::fdo::DBusProxy<'_>) -> Result<()> {
    let mut owners = dbus.receive_name_owner_changed().await.map_err(zbus_err)?;
    while let Some(sig) = owners.next().await {
        if let Ok(args) = sig.args() {
            if args.name().as_str().starts_with(MPRIS_PREFIX) && !args.new_owner().is_none() {
                return Ok(());
            }
        }
    }
    Err(Error::other("D-Bus name-owner stream ended"))
}

// --- zvariant metadata helpers ---

fn md_string(md: &HashMap<String, OwnedValue>, key: &str) -> Option<String> {
    let v = md.get(key)?;
    String::try_from(v.clone()).ok()
}

fn md_string_list(md: &HashMap<String, OwnedValue>, key: &str) -> Option<Vec<String>> {
    let v = md.get(key)?;
    Vec::<String>::try_from(v.clone()).ok()
}

fn md_i64(md: &HashMap<String, OwnedValue>, key: &str) -> Option<i64> {
    let v = md.get(key)?;
    if let Ok(x) = i64::try_from(v.clone()) {
        return Some(x);
    }
    u64::try_from(v.clone()).ok().map(|x| x as i64)
}

fn zbus_err(e: zbus::Error) -> Error {
    Error::other(format!("mpris: {e}"))
}
