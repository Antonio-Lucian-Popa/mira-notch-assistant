//! Local persistence: `config.toml` for settings and a SQLite database for
//! structured data (shelf paths, todos, water log — mostly phase 2).
//!
//! Data lives in the user's data directory:
//! * Windows: `%APPDATA%\Mira`
//! * Linux:   `~/.local/share/mira`
//!
//! Only the shelf's *paths* are stored, never file contents.

use std::path::{Path, PathBuf};

use rusqlite::{Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};

/// UI language.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Language {
    #[default]
    Ro,
    En,
}

/// Notch presentation mode (chiefly meaningful on the GNOME extension).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum NotchMode {
    /// Replace the panel clock and expand downward on hover.
    Integrated,
    /// A separate centered pill.
    #[default]
    Floating,
}

/// User settings, persisted as `config.toml`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub language: Language,
    pub mode: NotchMode,
    /// Collapse the closed notch to a thin line.
    pub minimal: bool,
    /// Use the always-animated eyes as the notch's resting face.
    pub mascot: bool,
    /// Launch on login.
    pub autostart: bool,
    /// Manual size multiplier on top of the wide-monitor auto scale.
    pub size_scale: f32,

    // --- phase 2 feature settings ---
    /// Daily water goal, in glasses.
    pub water_goal: u32,
    pub pomodoro_work_min: u32,
    pub pomodoro_short_break_min: u32,
    pub pomodoro_long_break_min: u32,
    pub pomodoro_cycles_before_long: u32,
    pub mascot_theme: String,
    pub mascot_contextual: bool,
    pub motion_intensity: f32,
    pub daily_summary_enabled: bool,
    pub daily_summary_time: String,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            language: Language::default(),
            mode: NotchMode::default(),
            minimal: false,
            mascot: false,
            autostart: false,
            size_scale: 1.0,
            water_goal: 8,
            pomodoro_work_min: 25,
            pomodoro_short_break_min: 5,
            pomodoro_long_break_min: 15,
            pomodoro_cycles_before_long: 4,
            mascot_theme: "classic".into(),
            mascot_contextual: true,
            motion_intensity: 0.6,
            daily_summary_enabled: true,
            daily_summary_time: "21:00".into(),
        }
    }
}

impl Config {
    /// Build the pomodoro engine config from these settings.
    pub fn pomodoro_config(&self) -> crate::pomodoro::PomodoroConfig {
        crate::pomodoro::PomodoroConfig {
            work_secs: self.pomodoro_work_min as i64 * 60,
            short_break_secs: self.pomodoro_short_break_min as i64 * 60,
            long_break_secs: self.pomodoro_long_break_min as i64 * 60,
            cycles_before_long: self.pomodoro_cycles_before_long.max(1),
        }
    }
}

impl Config {
    pub fn to_toml(&self) -> Result<String> {
        Ok(toml::to_string_pretty(self)?)
    }

    pub fn from_toml(s: &str) -> Result<Self> {
        Ok(toml::from_str(s)?)
    }

    /// Load from `path`, or return defaults if the file does not exist.
    pub fn load_or_default(path: &Path) -> Result<Self> {
        match std::fs::read_to_string(path) {
            Ok(s) => Config::from_toml(&s),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Config::default()),
            Err(e) => Err(Error::Io(e)),
        }
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(path, self.to_toml()?)?;
        Ok(())
    }
}

/// Resolve and create Mira's data directory.
pub fn data_dir() -> Result<PathBuf> {
    // `dirs::data_dir()` yields %APPDATA% on Windows and ~/.local/share on
    // Linux; we append the app folder (lowercase on Linux, "Mira" on Windows
    // by convention — we use "Mira" everywhere for a single code path).
    let base = dirs::data_dir().ok_or(Error::NoDataDir)?;
    let dir = if cfg!(windows) {
        base.join("Mira")
    } else {
        base.join("mira")
    };
    std::fs::create_dir_all(&dir)?;
    Ok(dir)
}

pub fn config_path() -> Result<PathBuf> {
    Ok(data_dir()?.join("config.toml"))
}

pub fn db_path() -> Result<PathBuf> {
    Ok(data_dir()?.join("mira.db"))
}

/// Handle to the SQLite database, with the schema applied.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, Default)]
pub struct DailySummary {
    pub day: String,
    pub focus_seconds: i64,
    pub focus_sessions: u32,
    pub todos_completed: u32,
    pub reminders_completed: u32,
    pub water_glasses: u32,
}
pub fn local_day(epoch: i64) -> String {
    use chrono::TimeZone;
    chrono::Local
        .timestamp_opt(epoch, 0)
        .single()
        .map(|t| t.format("%Y-%m-%d").to_string())
        .unwrap_or_else(|| "1970-01-01".into())
}

pub struct Store {
    conn: Connection,
}

impl Store {
    /// Open (creating if needed) the database at `path` and run migrations.
    pub fn open(path: &Path) -> Result<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let conn = Connection::open(path)?;
        Self::init(conn)
    }

    /// In-memory database — used by tests and the minimal fallback.
    pub fn open_in_memory() -> Result<Self> {
        Self::init(Connection::open_in_memory()?)
    }

    fn init(conn: Connection) -> Result<Self> {
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "foreign_keys", "ON")?;
        conn.execute_batch(
            r#"
            CREATE TABLE IF NOT EXISTS shelf (
                id          INTEGER PRIMARY KEY AUTOINCREMENT,
                path        TEXT NOT NULL,
                added_epoch INTEGER NOT NULL
            );
            CREATE TABLE IF NOT EXISTS todo (
                id              INTEGER PRIMARY KEY AUTOINCREMENT,
                text            TEXT NOT NULL,
                done            INTEGER NOT NULL DEFAULT 0,
                remind_at_epoch INTEGER
            );
            CREATE TABLE IF NOT EXISTS reminder (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                text TEXT NOT NULL,
                due_epoch INTEGER NOT NULL,
                created_epoch INTEGER NOT NULL,
                notified INTEGER NOT NULL DEFAULT 0
            );
            CREATE INDEX IF NOT EXISTS reminder_due ON reminder(due_epoch, id);
            CREATE TABLE IF NOT EXISTS app_meta (key TEXT PRIMARY KEY, value TEXT NOT NULL);
            CREATE TABLE IF NOT EXISTS activity_day (
                day TEXT PRIMARY KEY, focus_seconds INTEGER NOT NULL DEFAULT 0,
                focus_sessions INTEGER NOT NULL DEFAULT 0
            );
            CREATE TABLE IF NOT EXISTS completion_event (
                day TEXT NOT NULL, kind TEXT NOT NULL, entity INTEGER NOT NULL,
                slot INTEGER NOT NULL DEFAULT 0, PRIMARY KEY(day,kind,entity,slot)
            );
            CREATE TABLE IF NOT EXISTS water_log (
                day     TEXT PRIMARY KEY,
                glasses INTEGER NOT NULL DEFAULT 0
            );
            "#,
        )?;
        let columns: Vec<String> = conn
            .prepare("PRAGMA table_info(reminder)")?
            .query_map([], |row| row.get(1))?
            .collect::<std::result::Result<_, _>>()?;
        if !columns.iter().any(|name| name == "recurrence") {
            conn.execute("ALTER TABLE reminder ADD COLUMN recurrence TEXT", [])?;
        }
        // Upgrade Linux's original UTC-day-number water keys to calendar dates.
        let legacy: Vec<(String, u32)> = conn
            .prepare("SELECT day, glasses FROM water_log WHERE day NOT LIKE '%-%'")?
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
            .collect::<std::result::Result<_, _>>()?;
        for (key, glasses) in legacy {
            if let Ok(day) = key.parse::<i64>() {
                if let Some(epoch) = day.checked_mul(86400) {
                    let date = local_day(epoch);
                    conn.execute("INSERT INTO water_log(day,glasses) VALUES(?1,?2) ON CONFLICT(day) DO UPDATE SET glasses=MAX(glasses,excluded.glasses)", (&date,glasses))?;
                    conn.execute("DELETE FROM water_log WHERE day=?1", [&key])?;
                }
            }
        }
        Ok(Store { conn })
    }

    /// Append a path to the shelf, returning its row id. Stores the path only.
    pub fn add_shelf_item(&self, path: &str, added_epoch: i64) -> Result<i64> {
        self.conn.execute(
            "INSERT INTO shelf (path, added_epoch) VALUES (?1, ?2)",
            (path, added_epoch),
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    pub fn shelf_items(&self) -> Result<Vec<crate::model::ShelfItem>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, path, added_epoch FROM shelf ORDER BY added_epoch DESC, id DESC",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok(crate::model::ShelfItem {
                id: row.get(0)?,
                path: row.get(1)?,
                added_epoch: row.get(2)?,
            })
        })?;
        Ok(rows.collect::<std::result::Result<_, _>>()?)
    }

    pub fn remove_shelf_item(&self, id: i64) -> Result<()> {
        self.conn.execute("DELETE FROM shelf WHERE id = ?1", [id])?;
        Ok(())
    }

    // --- Todo ---

    pub fn add_todo(&self, text: &str) -> Result<i64> {
        self.conn
            .execute("INSERT INTO todo (text, done) VALUES (?1, 0)", [text])?;
        Ok(self.conn.last_insert_rowid())
    }

    pub fn todos(&self) -> Result<Vec<crate::model::Todo>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, text, done, remind_at_epoch FROM todo ORDER BY done ASC, id ASC",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok(crate::model::Todo {
                id: row.get(0)?,
                text: row.get(1)?,
                done: row.get::<_, i64>(2)? != 0,
                remind_at_epoch: row.get(3)?,
            })
        })?;
        Ok(rows.collect::<std::result::Result<_, _>>()?)
    }

    pub fn set_todo_done(&self, id: i64, done: bool) -> Result<()> {
        self.set_todo_done_at(id, done, chrono::Local::now().timestamp())
    }
    pub fn set_todo_done_at(&self, id: i64, done: bool, now: i64) -> Result<()> {
        let tx = self.conn.unchecked_transaction()?;
        let changed = tx.execute("UPDATE todo SET done=?2 WHERE id=?1", (id, done as i64))?;
        if changed > 0 {
            if done {
                tx.execute("INSERT OR IGNORE INTO completion_event(day,kind,entity,slot) VALUES(?1,'todo',?2,0)",(local_day(now),id))?;
            } else {
                tx.execute(
                    "DELETE FROM completion_event WHERE day=?1 AND kind='todo' AND entity=?2",
                    (local_day(now), id),
                )?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    pub fn remove_todo(&self, id: i64) -> Result<()> {
        self.conn.execute("DELETE FROM todo WHERE id = ?1", [id])?;
        Ok(())
    }

    /// Set (or clear, with `None`) a todo's reminder time.
    pub fn set_todo_reminder(&self, id: i64, remind_at_epoch: Option<i64>) -> Result<()> {
        self.conn.execute(
            "UPDATE todo SET remind_at_epoch = ?2 WHERE id = ?1",
            (id, remind_at_epoch),
        )?;
        Ok(())
    }

    /// Todos whose reminder is due at or before `now_epoch` and not yet done.
    pub fn due_reminders(&self, now_epoch: i64) -> Result<Vec<crate::model::Todo>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, text, done, remind_at_epoch FROM todo \
             WHERE remind_at_epoch IS NOT NULL AND remind_at_epoch <= ?1 AND done = 0",
        )?;
        let rows = stmt.query_map([now_epoch], |row| {
            Ok(crate::model::Todo {
                id: row.get(0)?,
                text: row.get(1)?,
                done: row.get::<_, i64>(2)? != 0,
                remind_at_epoch: row.get(3)?,
            })
        })?;
        Ok(rows.collect::<std::result::Result<_, _>>()?)
    }

    // --- Standalone reminders (acknowledged reminders are removed) ---
    pub fn add_reminder(&self, text: &str, due: i64, now: i64) -> Result<i64> {
        let text = crate::reminder::validate(text, due, now)?;
        self.conn.execute(
            "INSERT INTO reminder (text, due_epoch, created_epoch) VALUES (?1, ?2, ?3)",
            (text, due, now),
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    pub fn reminders(&self) -> Result<Vec<crate::reminder::Reminder>> {
        let mut stmt = self.conn.prepare("SELECT id, text, due_epoch, created_epoch, notified, recurrence FROM reminder ORDER BY due_epoch, id")?;
        let rows = stmt.query_map([], |row| {
            Ok(crate::reminder::Reminder {
                id: row.get(0)?,
                text: row.get(1)?,
                due_epoch: row.get(2)?,
                created_epoch: row.get(3)?,
                notified: row.get(4)?,
                recurrence: row
                    .get::<_, Option<String>>(5)?
                    .map(|json| {
                        serde_json::from_str(&json).map_err(|e| {
                            rusqlite::Error::FromSqlConversionFailure(
                                5,
                                rusqlite::types::Type::Text,
                                Box::new(e),
                            )
                        })
                    })
                    .transpose()?,
            })
        })?;
        Ok(rows.collect::<std::result::Result<_, _>>()?)
    }

    pub fn add_natural_reminder(&self, input: &str, now: i64) -> Result<i64> {
        let draft = crate::natural::parse_local(input, now)?;
        self.add_reminder_draft(&draft, now)
    }
    pub fn add_reminder_draft(&self, draft: &crate::natural::Draft, now: i64) -> Result<i64> {
        let title = crate::reminder::validate(&draft.title, draft.due, now)?;
        let recurrence = draft
            .recurrence
            .as_ref()
            .map(serde_json::to_string)
            .transpose()?;
        self.conn.execute(
            "INSERT INTO reminder(text,due_epoch,created_epoch,recurrence) VALUES(?1,?2,?3,?4)",
            (title, draft.due, now, recurrence),
        )?;
        Ok(self.conn.last_insert_rowid())
    }
    /// Complete this occurrence; a recurring series advances in local calendar
    /// time. Missed occurrences are skipped, not replayed in a notification storm.
    pub fn complete_reminder(&self, id: i64, now: i64) -> Result<()> {
        use chrono::TimeZone;
        let r = self
            .reminders()?
            .into_iter()
            .find(|r| r.id == id)
            .ok_or_else(|| crate::Error::other("Reminder no longer exists"))?;
        let next = if let Some(rule) = &r.recurrence {
            let anchor = chrono::Local
                .timestamp_opt(now.max(r.due_epoch), 0)
                .single()
                .ok_or_else(|| crate::Error::other("Dată invalidă"))?;
            Some(rule.next_after(anchor)?)
        } else {
            None
        };
        let tx = self.conn.unchecked_transaction()?;
        tx.execute("INSERT OR IGNORE INTO completion_event(day,kind,entity,slot) VALUES(?1,'reminder',?2,?3)", (local_day(now),id,r.due_epoch))?;
        if let Some(next) = next {
            tx.execute(
                "UPDATE reminder SET due_epoch=?2,created_epoch=?3,notified=0 WHERE id=?1",
                (id, next, now),
            )?;
        } else {
            tx.execute("DELETE FROM reminder WHERE id=?1", [id])?;
        }
        tx.commit()?;
        Ok(())
    }
    pub fn complete_occurrence(&self, id: i64, expected_due: i64, now: i64) -> Result<()> {
        if self
            .reminders()?
            .iter()
            .any(|r| r.id == id && r.due_epoch == expected_due)
        {
            self.complete_reminder(id, now)?;
        }
        Ok(())
    }
    pub fn get_meta(&self, key: &str) -> Result<Option<String>> {
        Ok(self
            .conn
            .query_row("SELECT value FROM app_meta WHERE key=?1", [key], |r| {
                r.get(0)
            })
            .optional()?)
    }
    pub fn set_meta(&self, key: &str, value: &str) -> Result<()> {
        self.conn.execute("INSERT INTO app_meta(key,value) VALUES(?1,?2) ON CONFLICT(key) DO UPDATE SET value=excluded.value",(key,value))?;
        Ok(())
    }
    pub fn record_focus(&self, now: i64, seconds: i64, completed: bool) -> Result<()> {
        self.record_focus_batch(now, seconds, completed as u32)
    }
    pub fn record_focus_batch(&self, now: i64, seconds: i64, sessions: u32) -> Result<()> {
        if seconds < 0 {
            return Err(crate::Error::other("Negative focus duration"));
        }
        self.conn.execute("INSERT INTO activity_day(day,focus_seconds,focus_sessions) VALUES(?1,?2,?3) ON CONFLICT(day) DO UPDATE SET focus_seconds=focus_seconds+excluded.focus_seconds,focus_sessions=focus_sessions+excluded.focus_sessions", (local_day(now),seconds,sessions))?;
        Ok(())
    }
    pub fn daily_summary(&self, day: &str) -> Result<DailySummary> {
        chrono::NaiveDate::parse_from_str(day, "%Y-%m-%d")
            .map_err(|_| crate::Error::other("Invalid calendar day"))?;
        let (focus_seconds, focus_sessions) = self
            .conn
            .query_row(
                "SELECT focus_seconds,focus_sessions FROM activity_day WHERE day=?1",
                [day],
                |r| Ok((r.get::<_, i64>(0)?, r.get::<_, u32>(1)?)),
            )
            .optional()?
            .unwrap_or_default();
        let count = |kind: &str| -> Result<u32> {
            Ok(self.conn.query_row(
                "SELECT COUNT(*) FROM completion_event WHERE day=?1 AND kind=?2",
                (day, kind),
                |r| r.get(0),
            )?)
        };
        Ok(DailySummary {
            day: day.into(),
            focus_seconds,
            focus_sessions,
            todos_completed: count("todo")?,
            reminders_completed: count("reminder")?,
            water_glasses: self.water_glasses(day)?,
        })
    }

    pub fn snooze_reminder(&self, id: i64, due: i64, now: i64) -> Result<()> {
        crate::reminder::validate("Snooze", due, now)?;
        let changed = self.conn.execute(
            "UPDATE reminder SET due_epoch=?2, created_epoch=?3, notified=0 WHERE id=?1",
            (id, due, now),
        )?;
        if changed == 0 {
            return Err(crate::Error::other("Reminder no longer exists"));
        }
        Ok(())
    }

    pub fn mark_reminder_notified(&self, id: i64) -> Result<()> {
        self.conn
            .execute("UPDATE reminder SET notified=1 WHERE id=?1", [id])?;
        Ok(())
    }

    pub fn remove_reminder(&self, id: i64) -> Result<()> {
        self.conn
            .execute("DELETE FROM reminder WHERE id=?1", [id])?;
        Ok(())
    }

    // --- Water (per day) ---

    /// Glasses logged for `day` (format "YYYY-MM-DD"); 0 if none.
    pub fn water_glasses(&self, day: &str) -> Result<u32> {
        let glasses: Option<i64> = self
            .conn
            .query_row(
                "SELECT glasses FROM water_log WHERE day = ?1",
                [day],
                |row| row.get(0),
            )
            .ok();
        Ok(glasses.unwrap_or(0).max(0) as u32)
    }

    /// Add `delta` (may be negative) to today's count, clamped at 0; returns new total.
    pub fn water_adjust(&self, day: &str, delta: i64) -> Result<u32> {
        let current = self.water_glasses(day)? as i64;
        let next = (current + delta).max(0);
        self.conn.execute(
            "INSERT INTO water_log (day, glasses) VALUES (?1, ?2) \
             ON CONFLICT(day) DO UPDATE SET glasses = ?2",
            (day, next),
        )?;
        Ok(next as u32)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_roundtrips_through_toml() {
        let cfg = Config {
            language: Language::En,
            mode: NotchMode::Integrated,
            minimal: true,
            size_scale: 1.25,
            ..Config::default()
        };

        let toml = cfg.to_toml().unwrap();
        let back = Config::from_toml(&toml).unwrap();
        assert_eq!(cfg, back);
    }

    #[test]
    fn config_load_default_when_missing() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("does-not-exist.toml");
        let cfg = Config::load_or_default(&path).unwrap();
        assert_eq!(cfg, Config::default());
    }

    #[test]
    fn config_save_then_load() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("sub").join("config.toml");
        let cfg = Config {
            autostart: true,
            language: Language::En,
            ..Config::default()
        };
        cfg.save(&path).unwrap();

        let loaded = Config::load_or_default(&path).unwrap();
        assert_eq!(cfg, loaded);
    }

    #[test]
    fn partial_toml_fills_defaults() {
        // A config file that only sets one field still parses.
        let cfg = Config::from_toml("language = \"en\"\n").unwrap();
        assert_eq!(cfg.language, Language::En);
        assert_eq!(cfg.size_scale, 1.0); // default preserved
    }

    #[test]
    fn shelf_crud_roundtrip() {
        let store = Store::open_in_memory().unwrap();
        let id1 = store.add_shelf_item("/home/u/a.pdf", 100).unwrap();
        let id2 = store.add_shelf_item("/home/u/b.png", 200).unwrap();

        let items = store.shelf_items().unwrap();
        assert_eq!(items.len(), 2);
        // Ordered newest-first.
        assert_eq!(items[0].id, id2);
        assert_eq!(items[0].path, "/home/u/b.png");

        store.remove_shelf_item(id1).unwrap();
        let items = store.shelf_items().unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].id, id2);
    }

    #[test]
    fn todo_crud_and_reminders() {
        let store = Store::open_in_memory().unwrap();
        let a = store.add_todo("Buy milk").unwrap();
        let b = store.add_todo("Call mom").unwrap();
        assert_eq!(store.todos().unwrap().len(), 2);

        store.set_todo_done(a, true).unwrap();
        let todos = store.todos().unwrap();
        // Done items sort after undone ones.
        assert_eq!(todos[0].id, b);
        assert!(todos.iter().find(|t| t.id == a).unwrap().done);

        // Reminders: due only when set, in the past, and not done.
        store.set_todo_reminder(b, Some(100)).unwrap();
        store.set_todo_reminder(a, Some(100)).unwrap(); // a is done -> excluded
        let due = store.due_reminders(150).unwrap();
        assert_eq!(due.len(), 1);
        assert_eq!(due[0].id, b);
        assert!(store.due_reminders(50).unwrap().is_empty()); // not yet due

        store.set_todo_reminder(b, None).unwrap();
        assert!(store.due_reminders(150).unwrap().is_empty());

        store.remove_todo(b).unwrap();
        assert_eq!(store.todos().unwrap().len(), 1);
    }

    #[test]
    fn water_adjust_clamps_and_persists() {
        let store = Store::open_in_memory().unwrap();
        let day = "2026-10-26";
        assert_eq!(store.water_glasses(day).unwrap(), 0);
        assert_eq!(store.water_adjust(day, 3).unwrap(), 3);
        assert_eq!(store.water_adjust(day, 1).unwrap(), 4);
        assert_eq!(store.water_glasses(day).unwrap(), 4);
        // Clamps at zero.
        assert_eq!(store.water_adjust(day, -10).unwrap(), 0);
        // A different day is independent.
        assert_eq!(store.water_glasses("2026-10-27").unwrap(), 0);
    }

    #[test]
    fn open_file_backed_db() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("mira.db");
        let store = Store::open(&path).unwrap();
        store.add_shelf_item("/x", 1).unwrap();
        drop(store);
        // Reopen and confirm persistence.
        let store = Store::open(&path).unwrap();
        assert_eq!(store.shelf_items().unwrap().len(), 1);
    }
}

#[cfg(test)]
mod reminder_tests {
    use super::*;
    #[test]
    fn reminders_survive_restart_and_snooze_rearms() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("mira.db");
        let id;
        {
            let store = Store::open(&path).unwrap();
            store.add_todo("A separate task").unwrap();
            id = store.add_reminder("  Call Mom  ", 200, 100).unwrap();
            store.add_reminder("Earlier", 150, 100).unwrap();
            store.mark_reminder_notified(id).unwrap();
        }
        let store = Store::open(&path).unwrap();
        let items = store.reminders().unwrap();
        assert_eq!(items[0].text, "Earlier");
        assert_eq!(items[1].text, "Call Mom");
        assert!(items[1].notified);
        store.snooze_reminder(id, 500, 200).unwrap();
        let item = &store.reminders().unwrap()[1];
        assert!(!item.notified);
        assert_eq!((item.due_epoch, item.created_epoch), (500, 200));
        store.remove_reminder(id).unwrap();
        assert_eq!(store.reminders().unwrap().len(), 1);
        assert_eq!(store.todos().unwrap().len(), 1);
        assert!(store.snooze_reminder(id, 700, 600).is_err());
    }
    #[test]
    fn invalid_reminders_do_not_change_database() {
        let store = Store::open_in_memory().unwrap();
        assert!(store.add_reminder("", 200, 100).is_err());
        assert!(store.add_reminder("Call", 100, 100).is_err());
        assert!(store.reminders().unwrap().is_empty());
    }
}

#[cfg(test)]
mod companion_tests {
    use super::*;
    use chrono::{Datelike, Local, TimeZone, Timelike};
    #[test]
    fn recurring_completion_is_idempotent_and_survives_restart() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("companion.db");
        let now = Local
            .with_ymd_and_hms(2026, 10, 27, 8, 0, 0)
            .unwrap()
            .timestamp();
        let id;
        let original_due;
        {
            let store = Store::open(&path).unwrap();
            id = store
                .add_natural_reminder("Raport în fiecare zi la 9", now)
                .unwrap();
            original_due = store.reminders().unwrap()[0].due_epoch;
            store
                .complete_occurrence(id, original_due, original_due)
                .unwrap();
            let next = store.reminders().unwrap()[0].due_epoch;
            store
                .complete_occurrence(id, original_due, original_due)
                .unwrap();
            assert_eq!(store.reminders().unwrap()[0].due_epoch, next);
            assert_eq!(
                store
                    .daily_summary(&local_day(now))
                    .unwrap()
                    .reminders_completed,
                1
            );
        }
        let store = Store::open(&path).unwrap();
        let r = store.reminders().unwrap().remove(0);
        let next = Local.timestamp_opt(r.due_epoch, 0).unwrap();
        assert_eq!((next.day(), next.hour()), (28, 9));
        assert!(r.recurrence.is_some());
        assert!(!r.notified);
        store.snooze_reminder(id, r.due_epoch + 300, now).unwrap();
        assert!(store.reminders().unwrap()[0].recurrence.is_some());
        store.remove_reminder(id).unwrap();
        assert!(store.reminders().unwrap().is_empty());
        assert_eq!(
            store
                .daily_summary(&local_day(now))
                .unwrap()
                .reminders_completed,
            1
        );
    }
    #[test]
    fn summary_uses_real_events_and_does_not_duplicate_task_completions() {
        let s = Store::open_in_memory().unwrap();
        let now = Local
            .with_ymd_and_hms(2026, 10, 27, 12, 0, 0)
            .unwrap()
            .timestamp();
        let day = local_day(now);
        let id = s.add_todo("Actual task").unwrap();
        s.set_todo_done_at(id, true, now).unwrap();
        s.set_todo_done_at(id, true, now).unwrap();
        s.record_focus(now, 60, true).unwrap();
        s.record_focus_batch(now, 120, 0).unwrap();
        s.water_adjust(&day, 3).unwrap();
        let report = s.daily_summary(&day).unwrap();
        assert_eq!(
            (
                report.focus_seconds,
                report.focus_sessions,
                report.todos_completed,
                report.water_glasses
            ),
            (180, 1, 1, 3)
        );
        s.set_todo_done_at(id, false, now).unwrap();
        assert_eq!(s.daily_summary(&day).unwrap().todos_completed, 0);
        assert_eq!(s.daily_summary("2026-10-28").unwrap().focus_seconds, 0);
        assert!(s.daily_summary("not-a-date").is_err());
    }
    #[test]
    fn old_database_schema_and_water_keys_migrate_without_losing_data() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("old.db");
        let conn = Connection::open(&path).unwrap();
        conn.execute_batch("CREATE TABLE reminder(id INTEGER PRIMARY KEY AUTOINCREMENT,text TEXT NOT NULL,due_epoch INTEGER NOT NULL,created_epoch INTEGER NOT NULL,notified INTEGER NOT NULL DEFAULT 0); INSERT INTO reminder(text,due_epoch,created_epoch) VALUES('Old reminder',200,100); CREATE TABLE water_log(day TEXT PRIMARY KEY, glasses INTEGER NOT NULL DEFAULT 0); INSERT INTO water_log VALUES('20753',4);").unwrap();
        drop(conn);
        let store = Store::open(&path).unwrap();
        assert_eq!(store.reminders().unwrap()[0].text, "Old reminder");
        assert!(store.reminders().unwrap()[0].recurrence.is_none());
        let migrated = local_day(20753 * 86400);
        assert_eq!(store.water_glasses(&migrated).unwrap(), 4);
        drop(store);
        assert_eq!(
            Store::open(&path)
                .unwrap()
                .water_glasses(&migrated)
                .unwrap(),
            4
        );
    }
}
