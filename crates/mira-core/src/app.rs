//! Central state aggregator.
//!
//! Hosts (the Windows app, the Linux daemon) keep an [`AppState`] and feed it
//! the events their sources produce. `AppState` holds the latest snapshot the
//! UI renders and owns the one piece of shared logic UIs must *not* duplicate:
//! deciding which lyric line is active. Keeping it here is what lets the GNOME
//! extension stay free of business logic — the daemon computes the active line
//! and signals the index.

use crate::event::AppEvent;
use crate::lyrics::Lyrics;
use crate::model::{NowPlaying, Power};

/// The complete snapshot the notch renders.
#[derive(Clone, Debug, Default)]
pub struct AppState {
    pub now_playing: NowPlaying,
    pub lyrics: Lyrics,
    pub power: Power,
    pub fullscreen: bool,
    /// Index of the currently highlighted lyric line, if any.
    pub active_line: Option<usize>,
}

impl AppState {
    pub fn new() -> Self {
        AppState::default()
    }

    /// Apply an inbound event, mutating state. Returns `true` when the visible
    /// state changed and a redraw is warranted — so a host only repaints when
    /// something actually moved.
    pub fn apply(&mut self, event: &AppEvent) -> bool {
        match event {
            AppEvent::NowPlaying(np) => {
                let changed = self.now_playing != *np;
                self.now_playing = np.clone();
                changed
            }
            AppEvent::Lyrics(ly) => {
                self.lyrics = ly.clone();
                // A new set of lyrics invalidates the highlighted line.
                self.active_line = self
                    .lyrics
                    .active_line_at(self.now_playing.live_position_micros());
                true
            }
            AppEvent::ActiveLine(idx) => {
                let changed = self.active_line != *idx;
                self.active_line = *idx;
                changed
            }
            AppEvent::Power(p) => {
                let changed = self.power != *p;
                self.power = *p;
                changed
            }
            AppEvent::Fullscreen(fs) => {
                let changed = self.fullscreen != *fs;
                self.fullscreen = *fs;
                changed
            }
            // Transient alerts don't alter persistent state; the host animates
            // them directly. They always warrant a draw.
            AppEvent::System(_) => true,
        }
    }

    /// Recompute the active lyric line from the live playback position. Hosts
    /// call this from a low-rate timer *only while playing with synced lyrics*.
    /// Returns `Some(new_index)` when the active line changed (so the host can
    /// emit `Lyrics.ActiveLineChanged`), else `None`.
    pub fn tick_active_line(&mut self) -> Option<Option<usize>> {
        if !self.lyrics.synced || !self.now_playing.status.is_playing() {
            return None;
        }
        let pos = self.now_playing.live_position_micros();
        let next = self.lyrics.active_line_at(pos);
        if next != self.active_line {
            self.active_line = next;
            Some(next)
        } else {
            None
        }
    }

    /// Whether the notch should currently be hidden (a fullscreen app is up).
    pub fn should_hide(&self) -> bool {
        self.fullscreen
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lyrics::LyricsSource;
    use crate::model::{PlaybackStatus, MICROS_PER_SEC};
    use std::time::Instant;

    fn playing_at(pos_micros: i64) -> NowPlaying {
        NowPlaying {
            status: PlaybackStatus::Playing,
            title: "T".into(),
            position_micros: pos_micros,
            length_micros: 300 * MICROS_PER_SEC,
            sampled_at: Some(Instant::now()),
            ..Default::default()
        }
    }

    #[test]
    fn apply_reports_change_only_on_difference() {
        let mut st = AppState::new();
        let np = playing_at(0);
        assert!(st.apply(&AppEvent::NowPlaying(np.clone())));
        // Applying the identical state again -> no change.
        assert!(!st.apply(&AppEvent::NowPlaying(np)));
    }

    #[test]
    fn fullscreen_toggles_hide() {
        let mut st = AppState::new();
        assert!(!st.should_hide());
        assert!(st.apply(&AppEvent::Fullscreen(true)));
        assert!(st.should_hide());
        assert!(!st.apply(&AppEvent::Fullscreen(true))); // unchanged
    }

    #[test]
    fn new_lyrics_set_initial_active_line() {
        let mut st = AppState::new();
        st.apply(&AppEvent::NowPlaying(playing_at(4 * MICROS_PER_SEC)));
        let lyrics = Lyrics::parse_lrc(
            "[00:01.00]A\n[00:03.00]B\n[00:05.00]C",
            LyricsSource::Lrclib,
        );
        st.apply(&AppEvent::Lyrics(lyrics));
        // At ~4s, line B (index 1) is active.
        assert_eq!(st.active_line, Some(1));
    }

    #[test]
    fn tick_active_line_emits_on_transition() {
        let mut st = AppState::new();
        st.apply(&AppEvent::NowPlaying(playing_at(0)));
        let lyrics = Lyrics::parse_lrc("[00:00.00]A\n[00:10.00]B", LyricsSource::Lrclib);
        st.apply(&AppEvent::Lyrics(lyrics));
        assert_eq!(st.active_line, Some(0));

        // Jump the position forward and re-tick.
        st.now_playing.position_micros = 10 * MICROS_PER_SEC;
        st.now_playing.sampled_at = Some(Instant::now());
        assert_eq!(st.tick_active_line(), Some(Some(1)));
        // No further change on a second tick at the same spot.
        assert_eq!(st.tick_active_line(), None);
    }

    #[test]
    fn tick_noop_when_paused() {
        let mut st = AppState::new();
        let mut np = playing_at(0);
        np.status = PlaybackStatus::Paused;
        st.apply(&AppEvent::NowPlaying(np));
        let lyrics = Lyrics::parse_lrc("[00:00.00]A", LyricsSource::Lrclib);
        st.apply(&AppEvent::Lyrics(lyrics));
        assert_eq!(st.tick_active_line(), None);
    }
}
