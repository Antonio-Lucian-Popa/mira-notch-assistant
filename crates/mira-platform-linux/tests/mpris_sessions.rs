#![cfg(target_os = "linux")]
//! Run on a private bus: dbus-run-session -- cargo test -p mira-platform-linux --test mpris_sessions
use mira_core::{
    event::{AppEvent, EventBus},
    source::{MediaControl, Source},
};
use mira_platform_linux::new_mpris;
use std::{collections::HashMap, time::Duration};
use zbus::{object_server::SignalEmitter, Connection, OwnedVariant};
const PATH: &str = "/org/mpris/MediaPlayer2";
struct MockPlayer {
    status: String,
    title: String,
    position: i64,
}
#[zbus::interface(name = "org.mpris.MediaPlayer2.Player")]
impl MockPlayer {
    #[zbus(property)]
    fn playback_status(&self) -> &str {
        &self.status
    }
    #[zbus(property)]
    fn metadata(&self) -> HashMap<String, OwnedVariant> {
        HashMap::from([
            (
                "xesam:title".into(),
                OwnedVariant::from(zbus::Str::from(self.title.clone())),
            ),
            (
                "xesam:artist".into(),
                OwnedVariant::from(zbus::Str::from("Artist")),
            ),
            ("mpris:length".into(), OwnedVariant::from(180_000_000i64)),
        ])
    }
    #[zbus(property)]
    fn position(&self) -> i64 {
        self.position
    }
    #[zbus(property)]
    fn can_play(&self) -> bool {
        true
    }
    #[zbus(property)]
    fn can_pause(&self) -> bool {
        true
    }
    #[zbus(property)]
    fn can_go_next(&self) -> bool {
        true
    }
    #[zbus(property)]
    fn can_go_previous(&self) -> bool {
        true
    }
    #[zbus(property)]
    fn can_seek(&self) -> bool {
        true
    }
    async fn play_pause(
        &mut self,
        #[zbus(signal_emitter)] e: SignalEmitter<'_>,
    ) -> zbus::fdo::Result<()> {
        self.status = if self.status == "Playing" {
            "Paused"
        } else {
            "Playing"
        }
        .into();
        self.playback_status_changed(&e)
            .await
            .map_err(|e| zbus::fdo::Error::Failed(e.to_string()))
    }
    #[zbus(signal)]
    async fn seeked(e: &SignalEmitter<'_>, position: i64) -> zbus::Result<()>;
}
async fn player(name: &str, status: &str, title: &str) -> Connection {
    zbus::connection::Builder::session()
        .name(name.to_string())
        .serve_at(
            PATH,
            MockPlayer {
                status: status.into(),
                title: title.into(),
                position: 0,
            },
        )
        .build()
        .await
        .unwrap()
}
async fn update(conn: &Connection, status: &str, title: &str) {
    let iface = conn
        .object_server()
        .interface::<_, MockPlayer>(PATH)
        .await
        .unwrap();
    let mut state = iface.get_mut().await;
    state.status = status.into();
    state.title = title.into();
    state
        .metadata_changed(iface.signal_emitter())
        .await
        .unwrap();
    state
        .playback_status_changed(iface.signal_emitter())
        .await
        .unwrap();
}
async fn expect(rx: &mut tokio::sync::broadcast::Receiver<AppEvent>, player: &str, title: &str) {
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if let AppEvent::NowPlaying(np) = rx.recv().await.unwrap() {
                if np.player_id == player && np.title == title {
                    break;
                }
            }
        }
    })
    .await
    .unwrap_or_else(|_| panic!("did not select {player}: {title}"));
}
#[tokio::test]
async fn switches_existing_browsers_handles_private_metadata_and_player_exit() {
    // Prevent an ordinary cargo test from connecting to the user's live bus.
    if std::env::var("MIRA_TEST_PRIVATE_BUS").as_deref() != Ok("1") {
        return;
    }
    let firefox_name = "org.mpris.MediaPlayer2.firefox.instance1";
    let brave_name = "org.mpris.MediaPlayer2.brave.instance2";
    let firefox = player(firefox_name, "Playing", "First song").await;
    let brave = player(brave_name, "Stopped", "").await;
    let bus = EventBus::default();
    let mut rx = bus.subscribe();
    let (source, control) = new_mpris(Connection::session().await.unwrap());
    let task = tokio::spawn(Box::new(source).run(bus));
    expect(&mut rx, firefox_name, "First song").await;
    // Both browsers already own their bus names: no NameOwnerChanged occurs.
    update(&firefox, "Paused", "First song").await;
    update(&brave, "Playing", "Brave song").await;
    expect(&mut rx, brave_name, "Brave song").await;
    update(&brave, "Playing", "A site is playing media").await;
    expect(&mut rx, brave_name, "A site is playing media").await;
    control.play_pause().await.unwrap();
    let state = brave
        .object_server()
        .interface::<_, MockPlayer>(PATH)
        .await
        .unwrap();
    assert_eq!(state.get().await.status, "Paused");
    brave.release_name(brave_name).await.unwrap();
    expect(&mut rx, firefox_name, "First song").await;
    firefox.release_name(firefox_name).await.unwrap();
    expect(&mut rx, "", "").await;
    task.abort();
}
