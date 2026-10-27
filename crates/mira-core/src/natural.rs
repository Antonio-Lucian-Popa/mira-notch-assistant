//! Deterministic Romanian/English reminder grammar. Local parsing, no AI/cloud.
use crate::{Error, Result};
use chrono::{DateTime, Datelike, Days, Local, LocalResult, NaiveDate, TimeZone};
use regex::Regex;
use serde::{Deserialize, Serialize};
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Recurrence {
    Daily {
        hour: u32,
        minute: u32,
    },
    Weekly {
        weekday: u32,
        hour: u32,
        minute: u32,
    },
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Draft {
    pub title: String,
    pub due: i64,
    pub recurrence: Option<Recurrence>,
    pub description: String,
}
fn regex(pattern: &str) -> Regex {
    Regex::new(pattern).expect("static reminder grammar")
}
fn clock(h: &str, m: Option<&str>) -> Result<(u32, u32)> {
    let h = h
        .parse::<u32>()
        .map_err(|_| Error::other("Ora este invalidă"))?;
    let m = m
        .unwrap_or("0")
        .parse::<u32>()
        .map_err(|_| Error::other("Minutul este invalid"))?;
    if h > 23 || m > 59 {
        return Err(Error::other("Folosește o oră între 00:00 și 23:59"));
    }
    Ok((h, m))
}
fn local_time<T: TimeZone>(tz: &T, day: NaiveDate, hour: u32, minute: u32) -> Result<DateTime<T>> {
    let naive = day
        .and_hms_opt(hour, minute, 0)
        .ok_or_else(|| Error::other("Ora nu există"))?;
    match tz.from_local_datetime(&naive) {
        LocalResult::Single(value) => Ok(value),
        LocalResult::Ambiguous(first, _) => Ok(first),
        LocalResult::None => Err(Error::other(
            "Ora nu există în fusul local din cauza schimbării orei; alege altă oră",
        )),
    }
}
impl Recurrence {
    pub fn next_after<T: TimeZone>(&self, now: DateTime<T>) -> Result<i64> {
        let (hour, minute, weekday) = match *self {
            Self::Daily { hour, minute } => (hour, minute, None),
            Self::Weekly {
                weekday,
                hour,
                minute,
            } => (hour, minute, Some(weekday)),
        };
        if hour > 23 || minute > 59 || weekday.is_some_and(|d| d > 6) {
            return Err(Error::other("Recurență invalidă"));
        }
        for offset in 0..15 {
            let day = now
                .date_naive()
                .checked_add_days(Days::new(offset))
                .ok_or_else(|| Error::other("Dată invalidă"))?;
            if weekday.is_some_and(|w| day.weekday().num_days_from_monday() != w) {
                continue;
            }
            // A nonexistent DST slot is skipped; the following occurrence keeps
            // the chosen wall clock time rather than drifting by 24h in UTC.
            if let Ok(next) = local_time(&now.timezone(), day, hour, minute) {
                if next > now {
                    return Ok(next.timestamp());
                }
            }
        }
        Err(Error::other("Nu am putut calcula următoarea apariție"))
    }
}
pub fn looks_natural(text: &str) -> bool {
    regex(r"(?i)\s(?:peste\s+(?:\d+|o|un)\b|in\s+(?:\d+|one)\b|m[âa]ine\b|tomorrow\b|every\b|(?:[îi]n\s+)?fiecare\b|(?:azi|today)\b|(?:la|at)\s+\d)").is_match(text)
}

pub fn parse_local(text: &str, now: i64) -> Result<Draft> {
    let now = Local
        .timestamp_opt(now, 0)
        .single()
        .ok_or_else(|| Error::other("Dată invalidă"))?;
    parse(text, now)
}
pub fn parse<T: TimeZone>(text: &str, now: DateTime<T>) -> Result<Draft> {
    let input = text.trim();
    let relative = regex(
        r"(?i)^(.*?)\s+(?:peste|in)\s+(\d+|o|un|one)\s*(?:de\s+)?(secund[ăa]|secunde|seconds?|s|minute?|minutes?|min|or[ăa]|ore|hours?|h|zi|zile|days?)\s*$",
    );
    if let Some(c) = relative.captures(input) {
        let amount = match c[2].to_lowercase().as_str() {
            "o" | "un" | "one" => 1,
            _ => c[2]
                .parse::<i64>()
                .map_err(|_| Error::other("Durată prea mare"))?,
        };
        let unit = c[3].to_lowercase();
        let multiplier = if unit.starts_with('s') {
            1
        } else if unit.starts_with('m') {
            60
        } else if unit.starts_with('o') || unit.starts_with('h') {
            3600
        } else {
            86400
        };
        let secs = amount
            .checked_mul(multiplier)
            .filter(|s| *s > 0 && *s <= 366 * 86400)
            .ok_or_else(|| Error::other("Alege o durată între o secundă și un an"))?;
        let title =
            crate::reminder::validate(c[1].trim(), now.timestamp() + secs, now.timestamp())?
                .to_owned();
        return Ok(Draft {
            title,
            due: now.timestamp() + secs,
            recurrence: None,
            description: format!("Peste {amount} {}", &c[3]),
        });
    }
    let recurring = regex(
        r"(?i)^(.*?)\s+(?:[îi]n\s+fiecare|fiecare|every)\s+(zi|day|luni|monday|mar[tț]i|tuesday|miercuri|wednesday|joi|thursday|vineri|friday|s[âa]mb[ăa]t[ăa]|saturday|duminic[ăa]|sunday)\s+(?:la|at)\s+(\d{1,2})(?::(\d{2}))?\s*$",
    );
    if let Some(c) = recurring.captures(input) {
        let (hour, minute) = clock(&c[3], c.get(4).map(|m| m.as_str()))?;
        let day = c[2].to_lowercase();
        let recurrence = if ["zi", "day"].contains(&day.as_str()) {
            Recurrence::Daily { hour, minute }
        } else {
            let normalized = day.replace(['ă', 'â'], "a").replace('ț', "t");
            let weekday = match normalized.as_str() {
                "luni" | "monday" => 0,
                "marți" | "marti" | "tuesday" => 1,
                "miercuri" | "wednesday" => 2,
                "joi" | "thursday" => 3,
                "vineri" | "friday" => 4,
                "sâmbătă" | "sambata" | "saturday" => 5,
                _ => 6,
            };
            Recurrence::Weekly {
                weekday,
                hour,
                minute,
            }
        };
        let due = recurrence.next_after(now.clone())?;
        let title = crate::reminder::validate(c[1].trim(), due, now.timestamp())?.to_owned();
        return Ok(Draft {
            title,
            due,
            recurrence: Some(recurrence),
            description: format!("În fiecare {day}, la {hour:02}:{minute:02}"),
        });
    }
    let absolute = regex(
        r"(?i)^(.*?)\s+(?:(m[âa]ine|tomorrow|azi|today)\s+)?(?:la|at)\s+(\d{1,2})(?::(\d{2}))?\s*$",
    );
    if let Some(c) = absolute.captures(input) {
        let (hour, minute) = clock(&c[3], c.get(4).map(|m| m.as_str()))?;
        let day_word = c
            .get(2)
            .map(|m| m.as_str().to_lowercase())
            .unwrap_or_default();
        let tomorrow = ["mâine", "maine", "tomorrow"].contains(&day_word.as_str());
        let mut day = now.date_naive();
        if tomorrow {
            day = day
                .succ_opt()
                .ok_or_else(|| Error::other("Dată invalidă"))?;
        }
        let mut due = local_time(&now.timezone(), day, hour, minute)?;
        if due <= now && day_word.is_empty() {
            due = local_time(
                &now.timezone(),
                day.succ_opt()
                    .ok_or_else(|| Error::other("Dată invalidă"))?,
                hour,
                minute,
            )?;
        }
        let title =
            crate::reminder::validate(c[1].trim(), due.timestamp(), now.timestamp())?.to_owned();
        return Ok(Draft {
            title,
            due: due.timestamp(),
            recurrence: None,
            description: due.naive_local().format("%d.%m.%Y %H:%M").to_string(),
        });
    }
    Err(Error::other("Exemple: «Sună-l pe Alex peste 20 de minute», «Raport mâine la 10», «Ședință în fiecare luni la 9»"))
}
#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{FixedOffset, Timelike, Utc};
    #[test]
    fn romanian_english_and_validation() {
        let now = Utc.with_ymd_and_hms(2026, 10, 27, 12, 0, 0).unwrap();
        let draft = parse("Sună-l pe Alex peste 20 de minute", now).unwrap();
        assert_eq!(draft.title, "Sună-l pe Alex");
        assert_eq!(draft.due, now.timestamp() + 1200);
        assert_eq!(
            parse("Call in one hour", now).unwrap().due,
            now.timestamp() + 3600
        );
        assert_eq!(
            parse("Raport mâine la 10", now).unwrap().due,
            now.timestamp() + 22 * 3600
        );
        assert!(parse("Raport azi la 10", now).is_err());
        for bad in [
            "Test peste 0 minute",
            "Test la 24:00",
            "Test mâine",
            "peste 5 minute",
        ] {
            assert!(parse(bad, now).is_err(), "{bad}");
        }
        let recurring = parse("Ședință în fiecare luni la 9", now).unwrap();
        assert!(matches!(
            recurring.recurrence,
            Some(Recurrence::Weekly { weekday: 0, .. })
        ));
        assert_eq!(
            parse("Pauză every day at 13:00", now).unwrap().due,
            now.timestamp() + 3600
        );
    }
    #[test]
    fn recurrence_uses_wall_time_and_skips_missed_occurrences() {
        let zone = FixedOffset::east_opt(3 * 3600).unwrap();
        let now = zone.with_ymd_and_hms(2026, 10, 27, 23, 58, 0).unwrap();
        let rule = Recurrence::Daily { hour: 0, minute: 0 };
        let next = zone
            .timestamp_opt(rule.next_after(now).unwrap(), 0)
            .unwrap();
        assert_eq!((next.day(), next.hour(), next.minute()), (28, 0, 0));
        assert!(Recurrence::Weekly {
            weekday: 9,
            hour: 0,
            minute: 0
        }
        .next_after(now)
        .is_err());
    }
}
