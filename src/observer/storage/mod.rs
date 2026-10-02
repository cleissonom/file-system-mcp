use super::metrics::Outcome;
use rusqlite::{Connection, OpenFlags};
use std::fs::File;
use std::io;
use std::path::Path;
use std::time::Duration;

mod migrations;
mod paths;
mod queries;

pub const SCHEMA_VERSION: u32 = migrations::MIGRATIONS.len() as u32;

pub struct Store {
    connection: Connection,
    // Closing another descriptor for the database can release SQLite's POSIX locks.
    _database: File,
    // Keep the separate advisory lock until SQLite closes and checkpoints its WAL.
    _lock: File,
}

pub struct CallRecord {
    pub sequence: u64,
    pub started_at_ms: u64,
    pub completed_at_ms: u64,
    pub tool: String,
    pub outcome: Outcome,
    pub duration_ms: f64,
    pub request_bytes: u64,
    pub response_bytes: u64,
}

#[derive(Default)]
pub struct Aggregate {
    pub calls: u64,
    pub successes: u64,
    pub errors: u64,
    pub duration_ms: f64,
}

pub struct ToolAggregate {
    pub tool: String,
    pub totals: Aggregate,
    pub last_called_at_ms: u64,
}

pub struct MinuteAggregate {
    pub minute_start_ms: u64,
    pub calls: u64,
    pub errors: u64,
    pub duration_ms: f64,
}

#[derive(Default)]
pub struct History {
    pub totals: Aggregate,
    pub request_bytes: u64,
    pub response_bytes: u64,
    pub last_sequence: u64,
    pub tools: Vec<ToolAggregate>,
    pub recent_calls: Vec<CallRecord>,
    pub minutes: Vec<MinuteAggregate>,
}

impl Store {
    pub fn open(path: &Path) -> io::Result<Self> {
        let prepared = paths::prepare(path)?;
        let flags = OpenFlags::SQLITE_OPEN_READ_WRITE
            | OpenFlags::SQLITE_OPEN_NO_MUTEX
            | OpenFlags::SQLITE_OPEN_NOFOLLOW;
        let mut connection =
            Connection::open_with_flags(&prepared.path, flags).map_err(sql_error)?;
        paths::verify_identity(&prepared.path, &prepared.database)?;
        initialize(&mut connection)?;
        paths::verify_sidecars(&prepared.path)?;
        Ok(Self {
            connection,
            _database: prepared.database,
            _lock: prepared.lock,
        })
    }

    pub fn append(&self, record: &CallRecord) -> io::Result<()> {
        queries::append(&self.connection, record)
    }

    pub fn load(&self, now_ms: u64) -> io::Result<History> {
        queries::load(&self.connection, now_ms)
    }

    pub fn schema_version(&self) -> u32 {
        SCHEMA_VERSION
    }
}

fn initialize(connection: &mut Connection) -> io::Result<()> {
    connection
        .busy_timeout(Duration::from_millis(250))
        .map_err(sql_error)?;
    connection
        .pragma_update(None, "trusted_schema", false)
        .map_err(sql_error)?;
    migrations::apply(connection, migrations::MIGRATIONS)?;
    connection
        .pragma_update(None, "journal_mode", "WAL")
        .map_err(sql_error)?;
    connection
        .pragma_update(None, "synchronous", "FULL")
        .map_err(sql_error)
}

fn sql_error(error: rusqlite::Error) -> io::Error {
    io::Error::other(error)
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod migration_tests;
