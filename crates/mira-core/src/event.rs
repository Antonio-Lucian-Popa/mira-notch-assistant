//! Internal event bus built on Tokio channels.
//!
//! Platform sources send [`AppEvent`]s into the bus; hosts subscribe and react
//! (update state, redraw, emit D-Bus signals). A `broadcast` channel gives us
//! cheap fan-out to several subscribers with no polling. Media control requests
//! flow the other way over a separate `mpsc` command channel.

use tokio::sync::{broadcast, mpsc};

use crate::lyrics::Lyrics;
use crate::model::{NowPlaying, Power, SystemEvent};

/// Something happened that the UI may care about.
#[derive(Clone, Debug)]
pub enum AppEvent {
    /// The now-playing state changed (new track, play/pause, seek, …).
    NowPlaying(NowPlaying),
    /// Lyrics for the current track were (re)loaded.
    Lyrics(Lyrics),
    /// Async lyric result; the host discards requests superseded by a newer track.
    LyricsResult { request_id: u64, lyrics: Lyrics },
    /// The active lyric line changed; `None` means "before the first line".
    /// Computed centrally so UIs stay logic-free.
    ActiveLine(Option<usize>),
    /// Battery / charger state changed.
    Power(Power),
    /// A transient system event worth a short alert.
    System(SystemEvent),
    /// A fullscreen app appeared (`true`) or went away (`false`); the notch
    /// hides while `true`.
    Fullscreen(bool),
}

/// A request from the UI to control the active media player.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MediaCommand {
    PlayPause,
    Next,
    Previous,
    /// Seek by a relative offset in microseconds (may be negative).
    Seek(i64),
    /// Seek to an absolute position in microseconds (used for lyric clicks).
    SetPosition(i64),
}

/// Fan-out event bus. Clone freely; all clones share the same channel.
#[derive(Clone)]
pub struct EventBus {
    tx: broadcast::Sender<AppEvent>,
}

impl EventBus {
    /// Create a bus whose backlog holds up to `capacity` events per subscriber.
    pub fn new(capacity: usize) -> Self {
        let (tx, _rx) = broadcast::channel(capacity);
        EventBus { tx }
    }

    /// Publish an event. Returns the number of subscribers that received it.
    /// An error (no subscribers) is intentionally ignored — producing events
    /// with nobody listening is fine.
    pub fn publish(&self, event: AppEvent) {
        let _ = self.tx.send(event);
    }

    /// Subscribe to future events.
    pub fn subscribe(&self) -> broadcast::Receiver<AppEvent> {
        self.tx.subscribe()
    }

    pub fn subscriber_count(&self) -> usize {
        self.tx.receiver_count()
    }
}

impl Default for EventBus {
    fn default() -> Self {
        // 64 events of backlog is ample: events are small and consumed promptly.
        EventBus::new(64)
    }
}

/// Creates a bounded media-command channel. The host holds the `rx` and drives
/// the active [`crate::source::MediaControl`]; the UI holds a [`CommandSender`].
pub fn command_channel(capacity: usize) -> (CommandSender, mpsc::Receiver<MediaCommand>) {
    let (tx, rx) = mpsc::channel(capacity);
    (CommandSender { tx }, rx)
}

#[derive(Clone)]
pub struct CommandSender {
    tx: mpsc::Sender<MediaCommand>,
}

impl CommandSender {
    /// Enqueue a command. If the channel is full or closed the command is
    /// dropped (the user can just press again); we never block the UI thread.
    pub fn send(&self, cmd: MediaCommand) {
        let _ = self.tx.try_send(cmd);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::PlaybackStatus;

    #[tokio::test]
    async fn broadcast_reaches_all_subscribers() {
        let bus = EventBus::new(8);
        let mut a = bus.subscribe();
        let mut b = bus.subscribe();
        assert_eq!(bus.subscriber_count(), 2);

        bus.publish(AppEvent::Fullscreen(true));

        assert!(matches!(
            a.recv().await.unwrap(),
            AppEvent::Fullscreen(true)
        ));
        assert!(matches!(
            b.recv().await.unwrap(),
            AppEvent::Fullscreen(true)
        ));
    }

    #[test]
    fn publish_without_subscribers_is_ok() {
        let bus = EventBus::new(4);
        bus.publish(AppEvent::Fullscreen(false)); // must not panic
    }

    #[tokio::test]
    async fn carries_now_playing_payload() {
        let bus = EventBus::default();
        let mut rx = bus.subscribe();
        let np = NowPlaying {
            status: PlaybackStatus::Playing,
            title: "Song".into(),
            ..NowPlaying::default()
        };
        bus.publish(AppEvent::NowPlaying(np));

        match rx.recv().await.unwrap() {
            AppEvent::NowPlaying(got) => {
                assert_eq!(got.title, "Song");
                assert!(got.status.is_playing());
            }
            _ => panic!("wrong event"),
        }
    }

    #[tokio::test]
    async fn command_channel_delivers() {
        let (tx, mut rx) = command_channel(4);
        tx.send(MediaCommand::PlayPause);
        tx.send(MediaCommand::SetPosition(1_000_000));
        assert_eq!(rx.recv().await.unwrap(), MediaCommand::PlayPause);
        assert_eq!(
            rx.recv().await.unwrap(),
            MediaCommand::SetPosition(1_000_000)
        );
    }
}
