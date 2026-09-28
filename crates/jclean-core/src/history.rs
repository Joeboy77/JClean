//! Local history in SQLite (spec §7.5, §10): scans, cleanups, and the
//! deletion log of every action taken.

use std::path::Path;
use std::sync::Mutex;

use rusqlite::{Connection, OptionalExtension, params};
use serde::Serialize;

use crate::time::{DAY_SECS, now_secs};

const SCHEMA_VERSION: i64 = 1;
const SCAN_RETENTION_DAYS: i64 = 180;

#[derive(Debug, thiserror::Error)]
pub enum HistoryError {
    #[error("history database error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("couldn't create {path}: {source}")]
    Io {
        path: String,
        #[source]
        source: std::io::Error,
    },
    #[error("history database is from a newer version of JClean")]
    TooNew,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ActionRecord {
    pub id: i64,
    pub cleanup_id: i64,
    pub time: i64,
    pub rule_id: String,
    pub path: String,
    pub method: String,
    pub bytes: i64,
    /// `cleaned`, `dry-run`, `skipped` or `failed`.
    pub outcome: String,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CleanupRecord {
    pub id: i64,
    pub scan_id: Option<i64>,
    pub time: i64,
    pub planned_bytes: i64,
    pub freed_bytes: Option<i64>,
    pub dry_run: bool,
}

pub struct NewAction<'a> {
    pub cleanup_id: i64,
    pub rule_id: &'a str,
    pub path: &'a str,
    pub method: &'a str,
    pub bytes: u64,
    pub outcome: &'a str,
    pub error: Option<&'a str>,
}

/// Thread-safe: the cleaner logs from several threads at once.
pub struct History {
    conn: Mutex<Connection>,
}

impl History {
    pub fn open(path: &Path) -> Result<Self, HistoryError> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|source| HistoryError::Io {
                path: parent.display().to_string(),
                source,
            })?;
        }
        Self::init(Connection::open(path)?)
    }

    pub fn open_in_memory() -> Result<Self, HistoryError> {
        Self::init(Connection::open_in_memory()?)
    }

    fn init(conn: Connection) -> Result<Self, HistoryError> {
        conn.execute_batch("PRAGMA foreign_keys = ON; PRAGMA journal_mode = WAL;")?;
        let version: i64 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
        if version > SCHEMA_VERSION {
            return Err(HistoryError::TooNew);
        }
        if version < 1 {
            conn.execute_batch(
                "CREATE TABLE IF NOT EXISTS scans (
                    id INTEGER PRIMARY KEY,
                    started INTEGER NOT NULL,
                    finished INTEGER NOT NULL,
                    mode TEXT NOT NULL,
                    total_bytes INTEGER NOT NULL,
                    reclaimable_bytes INTEGER NOT NULL,
                    item_count INTEGER NOT NULL
                );
                CREATE TABLE IF NOT EXISTS cleanups (
                    id INTEGER PRIMARY KEY,
                    scan_id INTEGER REFERENCES scans(id) ON DELETE SET NULL,
                    time INTEGER NOT NULL,
                    planned_bytes INTEGER NOT NULL,
                    freed_bytes INTEGER,
                    dry_run INTEGER NOT NULL
                );
                CREATE TABLE IF NOT EXISTS actions (
                    id INTEGER PRIMARY KEY,
                    cleanup_id INTEGER NOT NULL REFERENCES cleanups(id) ON DELETE CASCADE,
                    time INTEGER NOT NULL,
                    rule_id TEXT NOT NULL,
                    path TEXT NOT NULL,
                    method TEXT NOT NULL,
                    bytes INTEGER NOT NULL,
                    outcome TEXT NOT NULL,
                    error TEXT
                );
                CREATE INDEX IF NOT EXISTS actions_cleanup ON actions(cleanup_id);
                PRAGMA user_version = 1;",
            )?;
        }
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    fn with<T>(
        &self,
        f: impl FnOnce(&Connection) -> rusqlite::Result<T>,
    ) -> Result<T, HistoryError> {
        let conn = match self.conn.lock() {
            Ok(c) => c,
            Err(poisoned) => poisoned.into_inner(),
        };
        Ok(f(&conn)?)
    }

    pub fn record_scan(
        &self,
        started: i64,
        finished: i64,
        mode: &str,
        total: u64,
        reclaimable: u64,
        items: usize,
    ) -> Result<i64, HistoryError> {
        self.with(|c| {
            c.execute(
                "INSERT INTO scans (started, finished, mode, total_bytes, reclaimable_bytes, item_count) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![started, finished, mode, to_i64(total), to_i64(reclaimable), to_i64(items as u64)],
            )?;
            Ok(c.last_insert_rowid())
        })
    }

    pub fn start_cleanup(
        &self,
        scan_id: Option<i64>,
        planned: u64,
        dry_run: bool,
    ) -> Result<i64, HistoryError> {
        self.with(|c| {
            c.execute(
                "INSERT INTO cleanups (scan_id, time, planned_bytes, dry_run) VALUES (?1, ?2, ?3, ?4)",
                params![scan_id, now_secs(), to_i64(planned), dry_run],
            )?;
            Ok(c.last_insert_rowid())
        })
    }

    pub fn finish_cleanup(&self, cleanup_id: i64, freed: u64) -> Result<(), HistoryError> {
        self.with(|c| {
            c.execute(
                "UPDATE cleanups SET freed_bytes = ?1 WHERE id = ?2",
                params![to_i64(freed), cleanup_id],
            )?;
            Ok(())
        })
    }

    pub fn record_action(&self, a: &NewAction<'_>) -> Result<(), HistoryError> {
        self.with(|c| {
            c.execute(
                "INSERT INTO actions (cleanup_id, time, rule_id, path, method, bytes, outcome, error) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                params![a.cleanup_id, now_secs(), a.rule_id, a.path, a.method, to_i64(a.bytes), a.outcome, a.error],
            )?;
            Ok(())
        })
    }

    pub fn cleanups(&self) -> Result<Vec<CleanupRecord>, HistoryError> {
        self.with(|c| {
            let mut stmt = c.prepare("SELECT id, scan_id, time, planned_bytes, freed_bytes, dry_run FROM cleanups ORDER BY time DESC, id DESC")?;
            let rows = stmt.query_map([], |r| {
                Ok(CleanupRecord {
                    id: r.get(0)?,
                    scan_id: r.get(1)?,
                    time: r.get(2)?,
                    planned_bytes: r.get(3)?,
                    freed_bytes: r.get(4)?,
                    dry_run: r.get(5)?,
                })
            })?;
            rows.collect()
        })
    }

    pub fn actions(&self, cleanup_id: Option<i64>) -> Result<Vec<ActionRecord>, HistoryError> {
        self.with(|c| {
            let mut stmt = c.prepare(
                "SELECT id, cleanup_id, time, rule_id, path, method, bytes, outcome, error FROM actions
                 WHERE ?1 IS NULL OR cleanup_id = ?1 ORDER BY id",
            )?;
            let rows = stmt.query_map(params![cleanup_id], |r| {
                Ok(ActionRecord {
                    id: r.get(0)?,
                    cleanup_id: r.get(1)?,
                    time: r.get(2)?,
                    rule_id: r.get(3)?,
                    path: r.get(4)?,
                    method: r.get(5)?,
                    bytes: r.get(6)?,
                    outcome: r.get(7)?,
                    error: r.get(8)?,
                })
            })?;
            rows.collect()
        })
    }

    pub fn last_scan_id(&self) -> Result<Option<i64>, HistoryError> {
        self.with(|c| {
            c.query_row("SELECT id FROM scans ORDER BY id DESC LIMIT 1", [], |r| {
                r.get(0)
            })
            .optional()
        })
    }

    /// Drops scans older than 180 days. Cleanups and their logs are kept.
    pub fn prune(&self, now: i64) -> Result<usize, HistoryError> {
        let cutoff = now - SCAN_RETENTION_DAYS * DAY_SECS;
        self.with(|c| c.execute("DELETE FROM scans WHERE started < ?1", params![cutoff]))
    }

    /// The deletion log as CSV (spec §7.5).
    pub fn export_csv(&self) -> Result<String, HistoryError> {
        let mut out = String::from("time,cleanup_id,rule_id,path,method,bytes,outcome,error\n");
        for a in self.actions(None)? {
            let fields = [
                a.time.to_string(),
                a.cleanup_id.to_string(),
                a.rule_id,
                a.path,
                a.method,
                a.bytes.to_string(),
                a.outcome,
                a.error.unwrap_or_default(),
            ];
            out.push_str(
                &fields
                    .iter()
                    .map(|f| csv_field(f))
                    .collect::<Vec<_>>()
                    .join(","),
            );
            out.push('\n');
        }
        Ok(out)
    }
}

fn to_i64(n: u64) -> i64 {
    i64::try_from(n).unwrap_or(i64::MAX)
}

fn csv_field(s: &str) -> String {
    // Quote fields with separators, and neutralise spreadsheet formulas.
    let s = if s.starts_with(['=', '+', '-', '@']) {
        format!("'{s}")
    } else {
        s.to_string()
    };
    if s.contains([',', '"', '\n', '\r']) {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn records_and_exports_the_log() {
        let h = History::open_in_memory().unwrap();
        let scan = h.record_scan(1, 2, "quick", 100, 50, 3).unwrap();
        let cleanup = h.start_cleanup(Some(scan), 50, false).unwrap();
        h.record_action(&NewAction {
            cleanup_id: cleanup,
            rule_id: "macos.node.npm-cache",
            path: "/Users/me/a, \"b\"",
            method: "delete",
            bytes: 50,
            outcome: "cleaned",
            error: None,
        })
        .unwrap();
        h.finish_cleanup(cleanup, 48).unwrap();

        let cleanups = h.cleanups().unwrap();
        assert_eq!(cleanups.len(), 1);
        assert_eq!(cleanups[0].freed_bytes, Some(48));
        let csv = h.export_csv().unwrap();
        assert!(csv.contains("\"/Users/me/a, \"\"b\"\"\""));
    }

    #[test]
    fn pruning_keeps_cleanups() {
        let h = History::open_in_memory().unwrap();
        let old = h.record_scan(0, 1, "full", 1, 1, 1).unwrap();
        let cleanup = h.start_cleanup(Some(old), 1, false).unwrap();
        assert_eq!(h.prune(200 * DAY_SECS).unwrap(), 1);
        let cleanups = h.cleanups().unwrap();
        assert_eq!(cleanups[0].id, cleanup);
        assert_eq!(cleanups[0].scan_id, None);
    }
}
