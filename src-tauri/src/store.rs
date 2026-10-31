//! The only component that touches the database.

use std::{path::Path, sync::Mutex};

use rusqlite::{Connection, OptionalExtension, Result, params};
use serde::Serialize;
use serde_json::Value as Json;

use crate::sources::Reading;

const SCHEMA: &str = "
    CREATE TABLE IF NOT EXISTS current_values (
        source_id  TEXT NOT NULL,
        key        TEXT NOT NULL,
        value      TEXT NOT NULL,
        fetched_at INTEGER NOT NULL,
        PRIMARY KEY (source_id, key)
    ) WITHOUT ROWID;

    CREATE TABLE IF NOT EXISTS history (
        source_id  TEXT NOT NULL,
        key        TEXT NOT NULL,
        value      TEXT NOT NULL,
        fetched_at INTEGER NOT NULL,
        PRIMARY KEY (source_id, key, fetched_at)
    ) WITHOUT ROWID;

    CREATE TABLE IF NOT EXISTS events (
        id          INTEGER PRIMARY KEY AUTOINCREMENT,
        source_id   TEXT NOT NULL,
        type        TEXT NOT NULL,
        message     TEXT NOT NULL,
        occurred_at INTEGER NOT NULL
    );
    CREATE INDEX IF NOT EXISTS events_by_source ON events (source_id, id);

    CREATE TABLE IF NOT EXISTS settings (
        key   TEXT PRIMARY KEY,
        value TEXT NOT NULL
    ) WITHOUT ROWID;
";

pub const HISTORY_DAYS_SETTING: &str = "history_days";

/// Matches DEFAULT_HISTORY_DAYS in src/Settings.tsx.
const DEFAULT_HISTORY_DAYS: i64 = 14;
const MS_PER_DAY: i64 = 24 * 60 * 60 * 1000;

const EVENT_ERROR: &str = "error";
const EVENT_RECOVERED: &str = "recovered";

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Value {
    pub key: String,
    pub value: Json,
    pub fetched_at: i64,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct SourceError {
    pub message: String,
    pub occurred_at: i64,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Setting {
    pub key: String,
    pub value: Json,
}

pub struct Store {
    conn: Mutex<Connection>,
}

impl Store {
    pub fn open(path: &Path) -> Result<Self> {
        Self::init(Connection::open(path)?)
    }

    fn init(conn: Connection) -> Result<Self> {
        // WAL keeps a write every few seconds cheap: it appends rather than
        // syncing the whole database, and reads never wait on it.
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "synchronous", "NORMAL")?;
        conn.execute_batch(SCHEMA)?;
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    fn conn(&self) -> std::sync::MutexGuard<'_, Connection> {
        self.conn
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// Saves one run atomically, so the UI never sees half a run.
    pub fn save_run(&self, source_id: &str, readings: &[Reading], fetched_at: i64) -> Result<()> {
        let mut conn = self.conn();
        let tx = conn.transaction()?;
        {
            let mut current = tx.prepare_cached(
                "INSERT INTO current_values (source_id, key, value, fetched_at)
                 VALUES (?1, ?2, ?3, ?4)
                 ON CONFLICT (source_id, key)
                 DO UPDATE SET value = excluded.value, fetched_at = excluded.fetched_at",
            )?;
            let mut history = tx.prepare_cached(
                "INSERT OR REPLACE INTO history (source_id, key, value, fetched_at)
                 VALUES (?1, ?2, ?3, ?4)",
            )?;
            for reading in readings {
                let value = reading.value.to_string();
                current.execute(params![source_id, reading.key, value, fetched_at])?;
                if reading.keep_history {
                    history.execute(params![source_id, reading.key, value, fetched_at])?;
                }
            }
        }
        tx.commit()
    }

    pub fn current(&self, source_id: &str) -> Result<Vec<Value>> {
        let conn = self.conn();
        let mut statement = conn.prepare_cached(
            "SELECT key, value, fetched_at FROM current_values WHERE source_id = ?1 ORDER BY key",
        )?;
        let rows = statement.query_map([source_id], |row| {
            Ok(Value {
                key: row.get(0)?,
                value: parse(row.get(1)?),
                fetched_at: row.get(2)?,
            })
        })?;
        rows.collect()
    }

    pub fn current_number(&self, source_id: &str, key: &str) -> Result<Option<f64>> {
        let conn = self.conn();
        let value: Option<String> = conn
            .query_row(
                "SELECT value FROM current_values WHERE source_id = ?1 AND key = ?2",
                [source_id, key],
                |row| row.get(0),
            )
            .optional()?;
        Ok(value.and_then(|value| parse(value).as_f64()))
    }

    /// Values for a key between two times, oldest first.
    ///
    /// With `max_points`, numeric values are averaged into at most that many
    /// time buckets, so a chart of a long range stays cheap to send and draw.
    /// The stored history is untouched.
    pub fn history(
        &self,
        source_id: &str,
        key: &str,
        from: i64,
        to: i64,
        max_points: Option<u32>,
    ) -> Result<Vec<Value>> {
        let conn = self.conn();
        let map = |row: &rusqlite::Row| {
            Ok(Value {
                key: key.to_owned(),
                value: match row.get_ref(0)? {
                    rusqlite::types::ValueRef::Real(number) => number.into(),
                    other => parse(other.as_str()?.to_owned()),
                },
                fetched_at: row.get(1)?,
            })
        };

        match max_points {
            Some(points) => {
                let span = (to - from + 1).max(1);
                let bucket = (span + i64::from(points.max(1)) - 1) / i64::from(points.max(1));
                let mut statement = conn.prepare_cached(
                    "SELECT AVG(CAST(value AS REAL)), MAX(fetched_at) FROM history
                     WHERE source_id = ?1 AND key = ?2 AND fetched_at BETWEEN ?3 AND ?4
                     GROUP BY (fetched_at - ?3) / ?5
                     ORDER BY 2",
                )?;
                let rows = statement.query_map(params![source_id, key, from, to, bucket], map)?;
                rows.collect()
            }
            None => {
                let mut statement = conn.prepare_cached(
                    "SELECT value, fetched_at FROM history
                     WHERE source_id = ?1 AND key = ?2 AND fetched_at BETWEEN ?3 AND ?4
                     ORDER BY fetched_at",
                )?;
                let rows = statement.query_map(params![source_id, key, from, to], map)?;
                rows.collect()
            }
        }
    }

    pub fn add_error(&self, source_id: &str, message: &str, at: i64) -> Result<()> {
        self.add_event(source_id, EVENT_ERROR, message, at)
    }

    /// Records a recovery if the source's latest event was an error. Reading
    /// that from the events table means an error survives a restart and clears
    /// on the next good run.
    pub fn clear_error(&self, source_id: &str, at: i64) -> Result<()> {
        if self.active_error(source_id)?.is_some() {
            self.add_event(source_id, EVENT_RECOVERED, "Updating again", at)?;
        }
        Ok(())
    }

    /// The source's unresolved error, if its latest event is one.
    pub fn active_error(&self, source_id: &str) -> Result<Option<SourceError>> {
        let conn = self.conn();
        conn.query_row(
            "SELECT type, message, occurred_at FROM events
             WHERE source_id = ?1 ORDER BY id DESC LIMIT 1",
            [source_id],
            |row| {
                Ok(
                    (row.get::<_, String>(0)? == EVENT_ERROR).then_some(SourceError {
                        message: row.get(1)?,
                        occurred_at: row.get(2)?,
                    }),
                )
            },
        )
        .optional()
        .map(Option::flatten)
    }

    fn add_event(&self, source_id: &str, kind: &str, message: &str, at: i64) -> Result<()> {
        self.conn().execute(
            "INSERT INTO events (source_id, type, message, occurred_at) VALUES (?1, ?2, ?3, ?4)",
            params![source_id, kind, message, at],
        )?;
        Ok(())
    }

    /// Removes history older than the user chose to keep. Freed space is reused by
    /// new readings, so the file stops growing rather than shrinking.
    pub fn prune_history(&self, now: i64) -> Result<usize> {
        let days = self
            .setting(HISTORY_DAYS_SETTING)?
            .and_then(|days| days.as_i64())
            .unwrap_or(DEFAULT_HISTORY_DAYS);
        self.conn().execute(
            "DELETE FROM history WHERE fetched_at < ?1",
            [now - days * MS_PER_DAY],
        )
    }

    pub fn settings(&self) -> Result<Vec<Setting>> {
        let conn = self.conn();
        let mut statement = conn.prepare_cached("SELECT key, value FROM settings ORDER BY key")?;
        let rows = statement.query_map([], |row| {
            Ok(Setting {
                key: row.get(0)?,
                value: parse(row.get(1)?),
            })
        })?;
        rows.collect()
    }

    pub fn setting(&self, key: &str) -> Result<Option<Json>> {
        let value: Option<String> = self
            .conn()
            .query_row("SELECT value FROM settings WHERE key = ?1", [key], |row| {
                row.get(0)
            })
            .optional()?;
        Ok(value.map(parse))
    }

    pub fn set_setting(&self, key: &str, value: &Json) -> Result<()> {
        self.conn().execute(
            "INSERT INTO settings (key, value) VALUES (?1, ?2)
             ON CONFLICT (key) DO UPDATE SET value = excluded.value",
            params![key, value.to_string()],
        )?;
        Ok(())
    }
}

/// Everything in the value columns was written from valid JSON. If a row is
/// ever damaged by hand, show it as null rather than failing the whole read.
fn parse(text: String) -> Json {
    serde_json::from_str(&text).unwrap_or(Json::Null)
}
