//! Opening the database and keeping the schema up to date.
//!
//! Where the file lives is decided by the caller (`lib.rs` asks Tauri for the
//! per-app data directory) so that tests can open a throwaway database with
//! exactly the same setup as the real one.

use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::time::Duration;

use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions, SqliteSynchronous};
use sqlx::SqlitePool;

use crate::error::{AppError, Result};

/// The database file name inside the application data directory.
pub const DATABASE_FILE: &str = "tymio.db";

static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("./migrations");

/// The live connection pool, with the schema already migrated.
#[derive(Debug, Clone)]
pub struct Db {
    pool: SqlitePool,
}

impl Db {
    /// Opens (creating if needed) the database in `data_dir` and migrates it.
    pub async fn open_in(data_dir: &Path) -> Result<Self> {
        std::fs::create_dir_all(data_dir).map_err(|e| {
            AppError::Storage(format!("cannot create {}: {e}", data_dir.display()))
        })?;
        Self::open_file(&database_path(data_dir)).await
    }

    /// Opens a specific database file. Used by `open_in`, and directly by
    /// tests that want a real file on disk.
    pub async fn open_file(path: &Path) -> Result<Self> {
        let options = SqliteConnectOptions::new()
            .filename(path)
            .create_if_missing(true)
            // SQLite defaults foreign keys to OFF and this schema is heavily
            // relational; it has to be set per connection, not once.
            .foreign_keys(true)
            .journal_mode(SqliteJournalMode::Wal)
            .synchronous(SqliteSynchronous::Full)
            .busy_timeout(Duration::from_secs(5));

        let pool = SqlitePoolOptions::new().max_connections(4).connect_with(options).await?;
        Self::migrated(pool).await
    }

    /// A private, empty database that lives only as long as the pool.
    ///
    /// One connection only: a second connection to `:memory:` would get its
    /// own separate, empty database.
    pub async fn in_memory() -> Result<Self> {
        let options = SqliteConnectOptions::from_str("sqlite::memory:")?.foreign_keys(true);

        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .idle_timeout(None)
            .max_lifetime(None)
            .connect_with(options)
            .await?;
        Self::migrated(pool).await
    }

    async fn migrated(pool: SqlitePool) -> Result<Self> {
        MIGRATOR.run(&pool).await?;
        Ok(Db { pool })
    }

    pub fn pool(&self) -> &SqlitePool {
        &self.pool
    }

    /// Closes the pool. Restore and reset will need this before swapping the
    /// file underneath a running app.
    pub async fn close(&self) {
        self.pool.close().await;
    }
}

/// Where the database sits inside a given application data directory.
pub fn database_path(data_dir: &Path) -> PathBuf {
    data_dir.join(DATABASE_FILE)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn a_fresh_database_has_the_schema() {
        let db = Db::in_memory().await.expect("in-memory database opens");

        let tables: Vec<(String,)> =
            sqlx::query_as("SELECT name FROM sqlite_master WHERE type = 'table' ORDER BY name")
                .fetch_all(db.pool())
                .await
                .expect("schema is readable");
        let names: Vec<&str> = tables.iter().map(|(n,)| n.as_str()).collect();

        assert!(names.contains(&"projects"));
        assert!(names.contains(&"project_holidays"));
        assert!(names.contains(&"audit_log"));
    }

    #[tokio::test]
    async fn foreign_keys_are_enforced() {
        let db = Db::in_memory().await.expect("in-memory database opens");

        let (on,): (i64,) = sqlx::query_as("PRAGMA foreign_keys")
            .fetch_one(db.pool())
            .await
            .expect("pragma is readable");
        assert_eq!(on, 1, "SQLite defaults this off; the schema depends on it being on");

        let orphan = sqlx::query(
            "INSERT INTO project_holidays (id, project_id, date, name) VALUES (?1, ?2, ?3, ?4)",
        )
        .bind("h1")
        .bind("no-such-project")
        .bind("2026-01-01")
        .bind("New Year")
        .execute(db.pool())
        .await;

        assert!(orphan.is_err(), "a holiday cannot belong to a project that does not exist");
    }

    #[tokio::test]
    async fn migrating_twice_is_a_no_op() {
        let dir = tempfile::tempdir().expect("temp dir");
        let db = Db::open_in(dir.path()).await.expect("first open migrates");
        db.close().await;

        let reopened = Db::open_in(dir.path()).await.expect("second open re-migrates cleanly");
        let (count,): (i64,) = sqlx::query_as("SELECT count(*) FROM projects")
            .fetch_one(reopened.pool())
            .await
            .expect("projects table survives");
        assert_eq!(count, 0);
    }

    #[test]
    fn the_database_sits_directly_in_the_app_data_directory() {
        let path = database_path(Path::new("/Users/someone/Library/Application Support/io.tymio.hr"));
        assert!(path.ends_with("tymio.db"));
    }
}
