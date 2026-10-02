use super::sql_error;
use rusqlite::{Connection, TransactionBehavior};
use std::io;

pub(super) struct Migration {
    pub version: u32,
    pub sql: &'static str,
}

pub(super) const MIGRATIONS: &[Migration] = &[
    Migration {
        version: 1,
        sql: include_str!("migrations/0001_calls.sql"),
    },
    Migration {
        version: 2,
        sql: include_str!("migrations/0002_indexes.sql"),
    },
    Migration {
        version: 3,
        sql: include_str!("migrations/0003_origin.sql"),
    },
];

pub(super) fn apply(connection: &mut Connection, migrations: &[Migration]) -> io::Result<()> {
    validate_registry(migrations)?;
    let current: u32 = connection
        .pragma_query_value(None, "user_version", |row| row.get(0))
        .map_err(sql_error)?;
    let supported = migrations.last().map_or(0, |migration| migration.version);
    if current > supported {
        return Err(io::Error::other(
            "Observer database schema is newer than this server; upgrade the server before reopening it",
        ));
    }
    validate_database(connection, current)?;
    for migration in migrations
        .iter()
        .filter(|migration| migration.version > current)
    {
        apply_one(connection, migration)?;
    }
    Ok(())
}

fn validate_database(connection: &Connection, current: u32) -> io::Result<()> {
    let application_id: u32 = connection
        .pragma_query_value(None, "application_id", |row| row.get(0))
        .map_err(sql_error)?;
    if application_id == 0x4d43504f {
        return Ok(());
    }
    let tables: u32 = connection
        .query_row(
            "SELECT COUNT(*) FROM sqlite_schema WHERE name NOT LIKE 'sqlite_%'",
            [],
            |row| row.get(0),
        )
        .map_err(sql_error)?;
    if application_id == 0 && current == 0 && tables == 0 {
        return Ok(());
    }
    Err(io::Error::other(
        "This SQLite database belongs to another application; choose a new --dashboard-db instead",
    ))
}

fn validate_registry(migrations: &[Migration]) -> io::Result<()> {
    for (index, migration) in migrations.iter().enumerate() {
        if migration.version != index as u32 + 1 {
            return Err(io::Error::other(
                "Observer migration versions must be sequential starting at 1",
            ));
        }
    }
    Ok(())
}

fn apply_one(connection: &mut Connection, migration: &Migration) -> io::Result<()> {
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(sql_error)?;
    transaction
        .execute_batch(migration.sql)
        .map_err(sql_error)?;
    transaction
        .pragma_update(None, "user_version", migration.version)
        .map_err(sql_error)?;
    transaction.commit().map_err(sql_error)
}
