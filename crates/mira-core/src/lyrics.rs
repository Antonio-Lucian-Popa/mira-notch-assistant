//! LRC parsing and line⇄time synchronisation.
//!
//! The LRC format puts one or more time tags at the start of a line:
//! `[mm:ss.xx] text`. A line may carry several tags (`[t1][t2] text`),
//! meaning the same text appears at each time. Metadata tags such as
//! `[ar:Artist]`, `[ti:Title]`, `[length:03:21]` are ignored for timing.
//!
//! Lyrics with no time tags are kept as plain, unsynced text.

use serde::{Deserialize, Serialize};

use crate::model::MICROS_PER_SEC;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LyricsSource {
    #[default]
    None,
    Lrclib,
}

/// One timed lyric line.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LyricLine {
    pub time_micros: i64,
    pub text: String,
}

/// A parsed set of lyrics for a single track.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Lyrics {
    pub lines: Vec<LyricLine>,
    /// True when at least one line carries a timestamp.
    pub synced: bool,
    pub source: LyricsSource,
}

impl Lyrics {
    pub fn is_empty(&self) -> bool {
        self.lines.is_empty()
    }

    /// Parse synced LRC text. Lines without any time tag are dropped here;
    /// use [`Lyrics::from_plain`] for plain lyrics. Lines are returned sorted
    /// by time.
    pub fn parse_lrc(input: &str, source: LyricsSource) -> Self {
        let mut lines: Vec<LyricLine> = Vec::new();

        for raw in input.lines() {
            let (times, text) = split_time_tags(raw);
            if times.is_empty() {
                continue;
            }
            let text = text.trim().to_string();
            for time_micros in times {
                lines.push(LyricLine {
                    time_micros,
                    text: text.clone(),
                });
            }
        }

        // Stable sort so lines sharing a timestamp keep their input order.
        lines.sort_by_key(|l| l.time_micros);
        let synced = !lines.is_empty();
        Lyrics {
            lines,
            synced,
            source,
        }
    }

    /// Build unsynced lyrics from plain text (one entry per non-empty line,
    /// all at time 0).
    pub fn from_plain(input: &str, source: LyricsSource) -> Self {
        let lines: Vec<LyricLine> = input
            .lines()
            .map(str::trim)
            .filter(|l| !l.is_empty())
            .map(|l| LyricLine {
                time_micros: 0,
                text: l.to_string(),
            })
            .collect();
        Lyrics {
            lines,
            synced: false,
            source,
        }
    }

    /// Index of the line that should be highlighted at `position_micros`:
    /// the last line whose timestamp is `<= position`. `None` before the
    /// first line, or when there are no synced lines.
    pub fn active_line_at(&self, position_micros: i64) -> Option<usize> {
        if !self.synced || self.lines.is_empty() {
            return None;
        }
        // `partition_point` gives the count of lines with time <= position;
        // subtract one for the index. Returns None while position precedes
        // the very first line.
        let count = self
            .lines
            .partition_point(|l| l.time_micros <= position_micros);
        if count == 0 {
            None
        } else {
            Some(count - 1)
        }
    }
}

/// Parse the leading run of `[...]` tags on a line. Returns the microsecond
/// timestamps found (metadata tags are skipped) and the remaining text.
fn split_time_tags(line: &str) -> (Vec<i64>, &str) {
    let mut times = Vec::new();
    let mut rest = line;

    loop {
        let rest_trimmed = rest.trim_start();
        if !rest_trimmed.starts_with('[') {
            break;
        }
        let Some(close) = rest_trimmed.find(']') else {
            break;
        };
        let inner = &rest_trimmed[1..close];
        let after = &rest_trimmed[close + 1..];

        if let Some(micros) = parse_time_tag(inner) {
            times.push(micros);
            rest = after;
        } else {
            // A non-time tag. If we already collected time tags this is the
            // start of the text (rare, but keep it); otherwise it's metadata
            // and we skip just this tag.
            if times.is_empty() {
                rest = after;
            } else {
                rest = rest_trimmed;
                break;
            }
        }
    }

    (times, rest)
}

/// Parse a single time tag body like `mm:ss`, `mm:ss.xx` or `mm:ss.xxx`.
/// Returns `None` for metadata tags (e.g. `ar:...`).
fn parse_time_tag(inner: &str) -> Option<i64> {
    // A time tag must start with a digit and contain a colon separating
    // minutes from seconds. Metadata like "ti:..." starts with a letter.
    let (minutes_str, rest) = inner.split_once(':')?;
    let minutes: i64 = minutes_str.trim().parse().ok()?;

    // Seconds part may be "ss", "ss.xx" or "ss:xx" (some players use ':').
    let (secs_str, frac_str) = match rest.split_once(['.', ':']) {
        Some((s, f)) => (s, Some(f)),
        None => (rest, None),
    };
    let seconds: i64 = secs_str.trim().parse().ok()?;
    if !(0..60).contains(&seconds) {
        return None;
    }

    let frac_micros = match frac_str {
        None => 0,
        Some(f) => {
            let digits: String = f.chars().take_while(|c| c.is_ascii_digit()).collect();
            if digits.is_empty() {
                return None;
            }
            // Interpret the fraction at its own precision: ".5" = 500ms,
            // ".05" = 50ms, ".050" = 50ms, ".123" = 123ms.
            let value: i64 = digits.parse().ok()?;
            let scale = 10i64.pow(digits.len() as u32);
            value * MICROS_PER_SEC / scale
        }
    };

    Some(minutes * 60 * MICROS_PER_SEC + seconds * MICROS_PER_SEC + frac_micros)
}

#[cfg(test)]
mod tests {
    use super::*;

    const S: i64 = MICROS_PER_SEC;

    #[test]
    fn parses_basic_timed_lines() {
        let lrc = "[00:01.00]First line\n[00:02.50]Second line\n[01:00.00]Third";
        let lyrics = Lyrics::parse_lrc(lrc, LyricsSource::Lrclib);

        assert!(lyrics.synced);
        assert_eq!(lyrics.lines.len(), 3);
        assert_eq!(lyrics.lines[0].time_micros, S);
        assert_eq!(lyrics.lines[0].text, "First line");
        assert_eq!(lyrics.lines[1].time_micros, 2 * S + 500_000);
        assert_eq!(lyrics.lines[2].time_micros, 60 * S);
        assert_eq!(lyrics.lines[2].text, "Third");
    }

    #[test]
    fn expands_multiple_timestamps_on_one_line() {
        let lrc = "[00:10.00][00:20.00]Repeated chorus";
        let lyrics = Lyrics::parse_lrc(lrc, LyricsSource::Lrclib);

        assert_eq!(lyrics.lines.len(), 2);
        assert_eq!(lyrics.lines[0].time_micros, 10 * S);
        assert_eq!(lyrics.lines[1].time_micros, 20 * S);
        assert_eq!(lyrics.lines[0].text, "Repeated chorus");
        assert_eq!(lyrics.lines[1].text, "Repeated chorus");
    }

    #[test]
    fn ignores_metadata_tags() {
        let lrc = "[ar:Some Artist]\n[ti:A Song]\n[length:03:21]\n[00:05.00]Only real line";
        let lyrics = Lyrics::parse_lrc(lrc, LyricsSource::Lrclib);

        assert_eq!(lyrics.lines.len(), 1);
        assert_eq!(lyrics.lines[0].time_micros, 5 * S);
        assert_eq!(lyrics.lines[0].text, "Only real line");
    }

    #[test]
    fn handles_fraction_precision() {
        // ".5" -> 500ms, ".05" -> 50ms, ".123" -> 123ms (millis).
        assert_eq!(parse_time_tag("00:00.5"), Some(500_000));
        assert_eq!(parse_time_tag("00:00.05"), Some(50_000));
        assert_eq!(parse_time_tag("00:00.123"), Some(123_000));
        assert_eq!(parse_time_tag("00:01"), Some(S));
    }

    #[test]
    fn rejects_non_time_tag() {
        assert_eq!(parse_time_tag("ar:Artist"), None);
        assert_eq!(parse_time_tag("length:03:21"), None); // minutes "length" not numeric
    }

    #[test]
    fn plain_lyrics_are_unsynced() {
        let plain = "Line one\n\nLine two\n";
        let lyrics = Lyrics::from_plain(plain, LyricsSource::Lrclib);
        assert!(!lyrics.synced);
        assert_eq!(lyrics.lines.len(), 2);
        assert_eq!(lyrics.lines[1].text, "Line two");
    }

    #[test]
    fn active_line_selection_at_boundaries() {
        let lrc = "[00:01.00]A\n[00:03.00]B\n[00:05.00]C";
        let lyrics = Lyrics::parse_lrc(lrc, LyricsSource::Lrclib);

        // Before the first line.
        assert_eq!(lyrics.active_line_at(0), None);
        assert_eq!(lyrics.active_line_at(999_999), None);
        // Exactly on a timestamp -> that line is active.
        assert_eq!(lyrics.active_line_at(S), Some(0));
        // Between lines -> the earlier one stays active.
        assert_eq!(lyrics.active_line_at(2 * S), Some(0));
        assert_eq!(lyrics.active_line_at(3 * S), Some(1));
        assert_eq!(lyrics.active_line_at(4 * S), Some(1));
        // After the last line -> last line stays active.
        assert_eq!(lyrics.active_line_at(100 * S), Some(2));
    }

    #[test]
    fn active_line_none_when_unsynced() {
        let lyrics = Lyrics::from_plain("a\nb", LyricsSource::Lrclib);
        assert_eq!(lyrics.active_line_at(5 * S), None);
    }

    #[test]
    fn unsorted_input_is_sorted() {
        let lrc = "[00:05.00]C\n[00:01.00]A\n[00:03.00]B";
        let lyrics = Lyrics::parse_lrc(lrc, LyricsSource::Lrclib);
        let times: Vec<i64> = lyrics.lines.iter().map(|l| l.time_micros).collect();
        assert_eq!(times, vec![S, 3 * S, 5 * S]);
    }
}
