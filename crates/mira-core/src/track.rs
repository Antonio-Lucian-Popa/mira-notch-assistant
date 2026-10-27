//! Track identity and browser metadata normalization for lyric lookup.
use crate::model::NowPlaying;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TrackKey(pub String, pub String, pub String, pub String, pub i64);
impl TrackKey {
    pub fn of(np: &NowPlaying) -> Self {
        Self(
            np.player_id.clone(),
            np.title.clone(),
            np.artist.clone(),
            np.album.clone(),
            np.length_micros,
        )
    }
}

pub fn metadata_hidden(title: &str) -> bool {
    matches!(
        title.trim().to_lowercase().as_str(),
        "a site is playing media"
            | "media is playing"
            | "un site redă conținut media"
            | "un site redă media"
    )
}

pub fn clean_track(raw_title: &str, raw_artist: &str) -> (String, String) {
    let mut title = strip_noise(raw_title);
    let mut artist = strip_noise(raw_artist);
    if let Some(value) = artist.strip_suffix("- Topic") {
        artist = value.trim().to_string();
    }
    if artist.is_empty() {
        for separator in [" - ", " – ", " — "] {
            if let Some((a, t)) = title.split_once(separator) {
                artist = a.trim().to_string();
                title = t.trim().to_string();
                break;
            }
        }
    }
    (title, artist)
}

fn strip_noise(s: &str) -> String {
    let mut out = String::new();
    let mut chars = s.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '(' || ch == '[' {
            let closing = if ch == '(' { ')' } else { ']' };
            let mut segment = String::new();
            for next in chars.by_ref() {
                if next == closing {
                    break;
                }
                segment.push(next);
            }
            let lower = segment.to_lowercase();
            if ![
                "official",
                "lyrics",
                "lyric video",
                "music video",
                "visualizer",
                "hd",
                "4k",
            ]
            .iter()
            .any(|noise| lower.contains(noise))
            {
                out.push(ch);
                out.push_str(&segment);
                out.push(closing);
            }
        } else {
            out.push(ch);
        }
    }
    let out = out
        .trim()
        .trim_end_matches("- YouTube")
        .trim_end_matches("- Topic")
        .trim();
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn browser_titles_and_topic_channels() {
        assert_eq!(
            clean_track("Artist – Song (Official Video)", ""),
            ("Song".into(), "Artist".into())
        );
        assert_eq!(
            clean_track("Song [Lyrics] - YouTube", "Artist - Topic"),
            ("Song".into(), "Artist".into())
        );
        assert_eq!(
            clean_track("Song (Acoustic)", "Artist"),
            ("Song (Acoustic)".into(), "Artist".into())
        );
        assert!(metadata_hidden("A site is playing media"));
    }
    #[test]
    fn player_and_duration_are_part_of_identity() {
        let a = NowPlaying {
            title: "Song".into(),
            ..Default::default()
        };
        let mut b = a.clone();
        b.player_id = "brave".into();
        assert_ne!(TrackKey::of(&a), TrackKey::of(&b));
        b = a.clone();
        b.length_micros = 100;
        assert_ne!(TrackKey::of(&a), TrackKey::of(&b));
    }
}
