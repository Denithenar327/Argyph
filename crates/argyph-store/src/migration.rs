use rusqlite::Connection;

use crate::error::{Result, StoreError};

/// Ordered list of migrations. Each tuple is (name, SQL source).
const MIGRATIONS: &[(&str, &str)] = &[
    (
        "001_initial_files",
        include_str!("migrations/001_initial_files.sql"),
    ),
    (
        "002_symbols_chunks_edges",
        include_str!("migrations/002_symbols_chunks_edges.sql"),
    ),
    (
        "003_vectors",
        include_str!("migrations/003_vectors.sql"),
    ),
];

/// Run all pending migrations on `conn`. Idempotent — safe to call on every
/// boot.
pub fn run(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS _argyph_migrations (
            version INTEGER PRIMARY KEY,
            name    TEXT NOT NULL,
            applied_at TEXT NOT NULL DEFAULT (datetime('now'))
        );",
    )
    .map_err(|e| StoreError::Migration(format!("failed to create tracking table: {e}")))?;

    let current: i32 = conn
        .query_row(
            "SELECT COALESCE(MAX(version), 0) FROM _argyph_migrations",
            [],
            |row| row.get(0),
        )
        .unwrap_or(0);

    for (i, (name, sql)) in MIGRATIONS.iter().enumerate() {
        let version = (i + 1) as i32;
        if version > current {
            conn.execute_batch(sql)
                .map_err(|e| StoreError::Migration(format!("migration {name} failed: {e}")))?;
            conn.execute(
                "INSERT INTO _argyph_migrations (version, name) VALUES (?1, ?2)",
                rusqlite::params![version, name],
            )
            .map_err(|e| {
                StoreError::Migration(format!("failed to record migration {name}: {e}"))
            })?;
        }
    }

    Ok(())
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn run_is_idempotent() {
        let conn = Connection::open_in_memory().unwrap();
        run(&conn).unwrap();
        run(&conn).unwrap(); // second call must not fail

        let version: i32 = conn
            .query_row(
                "SELECT COALESCE(MAX(version), 0) FROM _argyph_migrations",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(version, 3);
    }
}
