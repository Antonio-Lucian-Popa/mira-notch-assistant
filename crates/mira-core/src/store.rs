//! Local persistence: `config.toml` for settings and a SQLite database for
//! structured data (shelf paths, todos, water log — mostly phase 2).
//!
//! Data lives in the user's data directory:
//! * Windows: `%APPDATA%\Mira`
//! * Linux:   `~/.local/share/mira`
//!
//! Only the shelf's *paths* are stored, never file contents.

use std::path::{Path, PathBuf};

use rusqlite::Connection;
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
    /// Enable the sleepy-eyes mascot (resting state) after inactivity.
    pub mascot: bool,
    /// Launch on login.
    pub autostart: bool,
    /// Manual size multiplier on top of the wide-monitor auto scale.
    pub size_scale: f32,
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
            CREATE TABLE IF NOT EXISTS water_log (
                day     TEXT PRIMARY KEY,
                glasses INTEGER NOT NULL DEFAULT 0
            );
            "#,
        )?;
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
