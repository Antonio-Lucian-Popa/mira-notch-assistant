//! Standalone reminders: independent of tasks, water and focus sessions.
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Reminder {
    pub id: i64,
    pub text: String,
    pub due_epoch: i64,
    pub created_epoch: i64,
    pub notified: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Urgency {
    Calm,
    Soon,
    Urgent,
    Due,
}

impl Reminder {
    pub fn remaining(&self, now: i64) -> i64 {
        self.due_epoch.saturating_sub(now).max(0)
    }
    pub fn urgency(&self, now: i64) -> Urgency {
        match self.due_epoch.saturating_sub(now) {
            ..=0 => Urgency::Due,
            1..=60 => Urgency::Urgent,
            61..=300 => Urgency::Soon,
            _ => Urgency::Calm,
        }
    }
    pub fn fraction(&self, now: i64) -> f64 {
        let total = self.due_epoch.saturating_sub(self.created_epoch).max(1);
        (self.remaining(now) as f64 / total as f64).clamp(0., 1.)
    }
}

pub fn countdown(seconds: i64) -> String {
    let seconds = seconds.max(0);
    if seconds >= 3600 {
        format!(
            "{}:{:02}:{:02}",
            seconds / 3600,
            seconds / 60 % 60,
            seconds % 60
        )
    } else {
        format!("{:02}:{:02}", seconds / 60, seconds % 60)
    }
}

pub fn validate(text: &str, due: i64, now: i64) -> crate::Result<&str> {
    let text = text.trim();
    if text.is_empty() || text.chars().count() > 200 {
        return Err(crate::Error::other(
            "Reminder title must contain 1–200 characters",
        ));
    }
    if due <= now || due.saturating_sub(now) > 366 * 86400 {
        return Err(crate::Error::other(
            "Choose a future time within the next year",
        ));
    }
    Ok(text)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn urgency_boundaries_and_clock_changes() {
        let r = Reminder {
            id: 1,
            text: "Call".into(),
            due_epoch: 1000,
            created_epoch: 0,
            notified: false,
        };
        for (now, expected) in [
            (699, Urgency::Calm),
            (700, Urgency::Soon),
            (939, Urgency::Soon),
            (940, Urgency::Urgent),
            (999, Urgency::Urgent),
            (1000, Urgency::Due),
            (1100, Urgency::Due),
        ] {
            assert_eq!(r.urgency(now), expected);
        }
        assert_eq!(r.fraction(-100), 1.);
        assert_eq!(r.fraction(500), 0.5);
        assert_eq!(r.fraction(1100), 0.);
    }
    #[test]
    fn rejects_invalid_reminders() {
        assert!(validate("  ", 100, 0).is_err());
        assert!(validate("Call", 100, 100).is_err());
        assert!(validate("Call", i64::MAX, 100).is_err());
        assert_eq!(validate("  Call  ", 200, 100).unwrap(), "Call");
    }
    #[test]
    fn countdown_keeps_hours_and_never_goes_negative() {
        assert_eq!(countdown(-1), "00:00");
        assert_eq!(countdown(65), "01:05");
        assert_eq!(countdown(3661), "1:01:01");
    }
}
