//! LRCLIB (<https://lrclib.net>) client — the *only* network access in Mira.
//!
//! We query `/api/get` by track metadata and duration; when that misses we
//! fall back to `/api/search`. The JSON→[`Lyrics`] mapping is factored out so
//! it can be unit-tested without touching the network.

use serde::Deserialize;

use crate::error::{Error, Result};
use crate::lyrics::{Lyrics, LyricsSource};

const API_BASE: &str = "https://lrclib.net";
const USER_AGENT: &str = concat!(
    "Mira/",
    env!("CARGO_PKG_VERSION"),
    " (https://github.com/antonio-lucian-popa/mira-notch-assistant)"
);

/// What we know about the track we want lyrics for.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TrackQuery {
    pub title: String,
    pub artist: String,
    pub album: String,
    /// Track length in whole seconds; LRCLIB matches within ±2s.
    pub duration_secs: Option<i64>,
}

impl TrackQuery {
    pub fn new(title: impl Into<String>, artist: impl Into<String>) -> Self {
        TrackQuery {
            title: title.into(),
            artist: artist.into(),
            ..Default::default()
        }
    }
}

/// Raw shape of an LRCLIB record (`/api/get` and items from `/api/search`).
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LrclibRecord {
    #[serde(default)]
    pub track_name: String,
    #[serde(default)]
    pub artist_name: String,
    #[serde(default)]
    pub duration: f64,
    #[serde(default)]
    pub instrumental: bool,
    #[serde(default)]
    pub plain_lyrics: Option<String>,
    #[serde(default)]
    pub synced_lyrics: Option<String>,
}

impl LrclibRecord {
    /// Convert a record into [`Lyrics`], preferring synced over plain.
    /// Returns [`Error::LyricsNotFound`] for instrumental or empty records.
    pub fn into_lyrics(self) -> Result<Lyrics> {
        if self.instrumental {
            return Err(Error::LyricsNotFound);
        }
        if let Some(synced) = self.synced_lyrics.filter(|s| !s.trim().is_empty()) {
            let lyrics = Lyrics::parse_lrc(&synced, LyricsSource::Lrclib);
            if !lyrics.is_empty() {
                return Ok(lyrics);
            }
        }
        if let Some(plain) = self.plain_lyrics.filter(|s| !s.trim().is_empty()) {
            let lyrics = Lyrics::from_plain(&plain, LyricsSource::Lrclib);
            if !lyrics.is_empty() {
                return Ok(lyrics);
            }
        }
        Err(Error::LyricsNotFound)
    }
}

/// Thin async client around the LRCLIB HTTP API.
#[derive(Clone)]
pub struct LrclibClient {
    http: reqwest::Client,
    base: String,
}

impl LrclibClient {
    pub fn new() -> Result<Self> {
        let http = reqwest::Client::builder()
            .user_agent(USER_AGENT)
            .connect_timeout(std::time::Duration::from_secs(5))
            .timeout(std::time::Duration::from_secs(12))
            .build()?;
        Ok(LrclibClient {
            http,
            base: API_BASE.to_string(),
        })
    }

    /// Override the base URL (for testing against a local mock server).
    pub fn with_base(mut self, base: impl Into<String>) -> Self {
        self.base = base.into();
        self
    }

    /// Fetch lyrics for a track: exact `/api/get` first, then `/api/search`.
    pub async fn fetch(&self, q: &TrackQuery) -> Result<Lyrics> {
        if let Ok(lyrics) = self.get(q).await {
            return Ok(lyrics);
        }
        self.search(q).await
    }

    async fn get(&self, q: &TrackQuery) -> Result<Lyrics> {
        if q.artist.trim().is_empty() {
            return Err(Error::LyricsNotFound);
        }
        let mut params: Vec<(&str, String)> = vec![
            ("track_name", q.title.clone()),
            ("artist_name", q.artist.clone()),
        ];
        if !q.album.is_empty() {
            params.push(("album_name", q.album.clone()));
        }
        if let Some(d) = q.duration_secs {
            params.push(("duration", d.to_string()));
        }

        let url = format!("{}/api/get", self.base);
        let resp = self.http.get(&url).query(&params).send().await?;
        if !resp.status().is_success() {
            return Err(Error::LyricsNotFound);
        }
        let record: LrclibRecord = resp.json().await?;
        record.into_lyrics()
    }

    async fn search(&self, q: &TrackQuery) -> Result<Lyrics> {
        let mut params: Vec<(&str, String)> = vec![("track_name", q.title.clone())];
        if !q.artist.trim().is_empty() {
            params.push(("artist_name", q.artist.clone()));
        }
        let url = format!("{}/api/search", self.base);
        let resp = self.http.get(&url).query(&params).send().await?;
        if !resp.status().is_success() {
            return Err(Error::LyricsNotFound);
        }
        let mut records: Vec<LrclibRecord> = resp.json().await?;
        rank_records(&mut records, q);
        // Take the first record that yields usable lyrics.
        records
            .into_iter()
            .filter(|r| r.track_name.is_empty() || canonical(&r.track_name) == canonical(&q.title))
            .find_map(|r| r.into_lyrics().ok())
            .ok_or(Error::LyricsNotFound)
    }
}

fn canonical(text: &str) -> String {
    text.to_lowercase()
        .chars()
        .filter(|ch| ch.is_alphanumeric())
        .collect()
}
fn rank_records(records: &mut [LrclibRecord], q: &TrackQuery) {
    records.sort_by_key(|r| {
        (
            canonical(&r.track_name) != canonical(&q.title),
            !q.artist.is_empty() && canonical(&r.artist_name) != canonical(&q.artist),
            q.duration_secs
                .map(|d| ((r.duration - d as f64).abs() * 1000.) as i64)
                .unwrap_or(0),
            r.synced_lyrics
                .as_ref()
                .map(|l| l.trim().is_empty())
                .unwrap_or(true),
        )
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn search_prefers_the_same_song_artist_and_duration() {
        let mut records: Vec<LrclibRecord> = serde_json::from_str(
            r#"[
          {"trackName":"Another song","artistName":"Artist","duration":180},
          {"trackName":"Song","artistName":"Cover band","duration":180},
          {"trackName":"Song","artistName":"Artist","duration":300},
          {"trackName":"Song","artistName":"Artist","duration":180}
        ]"#,
        )
        .unwrap();
        rank_records(
            &mut records,
            &TrackQuery {
                title: "Song".into(),
                artist: "Artist".into(),
                duration_secs: Some(180),
                ..Default::default()
            },
        );
        assert_eq!(
            (
                records[0].track_name.as_str(),
                records[0].artist_name.as_str(),
                records[0].duration
            ),
            ("Song", "Artist", 180.)
        );
        assert_eq!(canonical("Don't stop!"), canonical("Dont stop"));
    }

    #[test]
    fn maps_synced_record_to_timed_lyrics() {
        let json = r#"{
            "instrumental": false,
            "plainLyrics": "Hello\nWorld",
            "syncedLyrics": "[00:01.00]Hello\n[00:02.00]World"
        }"#;
        let record: LrclibRecord = serde_json::from_str(json).unwrap();
        let lyrics = record.into_lyrics().unwrap();
        assert!(lyrics.synced);
        assert_eq!(lyrics.lines.len(), 2);
    }

    #[test]
    fn falls_back_to_plain_when_no_synced() {
        let json = r#"{ "instrumental": false, "plainLyrics": "Just text", "syncedLyrics": null }"#;
        let record: LrclibRecord = serde_json::from_str(json).unwrap();
        let lyrics = record.into_lyrics().unwrap();
        assert!(!lyrics.synced);
        assert_eq!(lyrics.lines.len(), 1);
        assert_eq!(lyrics.lines[0].text, "Just text");
    }

    #[test]
    fn instrumental_has_no_lyrics() {
        let json = r#"{ "instrumental": true, "plainLyrics": null, "syncedLyrics": null }"#;
        let record: LrclibRecord = serde_json::from_str(json).unwrap();
        assert!(matches!(record.into_lyrics(), Err(Error::LyricsNotFound)));
    }

    #[test]
    fn empty_strings_count_as_missing() {
        let json = r#"{ "instrumental": false, "plainLyrics": "   ", "syncedLyrics": "" }"#;
        let record: LrclibRecord = serde_json::from_str(json).unwrap();
        assert!(matches!(record.into_lyrics(), Err(Error::LyricsNotFound)));
    }

    #[test]
    fn tolerates_extra_unknown_fields() {
        let json = r#"{
            "id": 42,
            "trackName": "X",
            "artistName": "Y",
            "duration": 180,
            "instrumental": false,
            "plainLyrics": "a",
            "syncedLyrics": null
        }"#;
        let record: LrclibRecord = serde_json::from_str(json).unwrap();
        assert!(record.into_lyrics().is_ok());
    }
}
