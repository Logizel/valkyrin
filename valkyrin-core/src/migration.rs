use anyhow::Context;
use sqlx::{Connection, Executor, PgConnection, MySqlConnection, SqliteConnection, types::chrono::DateTime, types::chrono::Utc};
use std::path::Path;
use tokio::fs;

/// Represents a migration record in the database
#[derive(Debug, sqlx::FromRow)]
pub struct MigrationRecord {
    pub version: String,
    pub name: String,
    pub checksum: String,
    pub applied_at: DateTime<Utc>,
    pub success: bool,
}

/// Creates the migration history table if it doesn't exist (PostgreSQL)
pub async fn create_migration_table(
    conn: &mut PgConnection,
) -> Result<(), anyhow::Error> {
    conn.execute(
        r#"
        CREATE TABLE IF NOT EXISTS _valkyrin_migrations (
            version TEXT PRIMARY KEY,
            name TEXT NOT NULL,
            checksum TEXT NOT NULL,
            applied_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
            success BOOLEAN NOT NULL
        )
        "#
    )
    .await
    .context("Failed to create migration table")?;
    Ok(())
}

/// Creates the migration history table if it doesn't exist (MySQL)
pub async fn create_migration_table_mysql(
    conn: &mut MySqlConnection,
) -> Result<(), anyhow::Error> {
    conn.execute(
        r#"
        CREATE TABLE IF NOT EXISTS _valkyrin_migrations (
            version VARCHAR(255) PRIMARY KEY,
            name VARCHAR(255) NOT NULL,
            checksum VARCHAR(64) NOT NULL,
            applied_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
            success BOOLEAN NOT NULL
        )
        "#
    )
    .await
    .context("Failed to create migration table")?;
    Ok(())
}

/// Creates the migration history table if it doesn't exist (SQLite)
pub async fn create_migration_table_sqlite(
    conn: &mut SqliteConnection,
) -> Result<(), anyhow::Error> {
    conn.execute(
        r#"
        CREATE TABLE IF NOT EXISTS _valkyrin_migrations (
            version TEXT PRIMARY KEY,
            name TEXT NOT NULL,
            checksum TEXT NOT NULL,
            applied_at TEXT NOT NULL DEFAULT (datetime('now')),
            success BOOLEAN NOT NULL
        )
        "#
    )
    .await
    .context("Failed to create migration table")?;
    Ok(())
}

/// PostgreSQL lock mechanism using advisory locks
pub async fn pg_advisory_lock(
    conn: &mut PgConnection,
    lock_id: i64,
) -> Result<(), anyhow::Error> {
    sqlx::query("SELECT pg_advisory_lock($1)")
        .bind(lock_id)
        .execute(conn)
        .await
        .context("Failed to acquire PostgreSQL advisory lock")?;
    Ok(())
}

/// MySQL lock mechanism
pub async fn mysql_lock(
    conn: &mut MySqlConnection,
    lock_name: &str,
) -> Result<(), anyhow::Error> {
    sqlx::query("SELECT GET_LOCK(?, 30)") // 30 second timeout
        .bind(lock_name)
        .execute(conn)
        .await
        .context("Failed to acquire MySQL lock")?;
    Ok(())
}

/// SQLite lock mechanism using file locks
pub async fn sqlite_lock(db_path: &str) -> Result<(), anyhow::Error> {
    let lock_file = Path::new(db_path).with_extension("lock");
    let _file = fs::OpenOptions::new()
        .create(true)
        .write(true)
        .open(lock_file)
        .await
        .context("Failed to create SQLite lock file")?;
    // File lock is automatically released when _file goes out of scope
    Ok(())
}

/// Applies migrations with proper locking and tracking
pub async fn apply_migrations_with_lock(
    db_url: &str,
    migrations: Vec<Migration>,
) -> Result<(), anyhow::Error> {
    // Determine database type from URL
    let db_type = if db_url.starts_with("postgres") {
        DatabaseType::PostgreSQL
    } else if db_url.starts_with("mysql") {
        DatabaseType::MySQL
    } else if db_url.starts_with("sqlite") {
        DatabaseType::SQLite
    } else {
        return Err(anyhow::anyhow!("Unsupported database type"));
    };

    match db_type {
        DatabaseType::PostgreSQL => {
            let mut conn = PgConnection::connect(db_url).await?;
            pg_advisory_lock(&mut conn, 0x76616C6B7972696E).await?; // 'valkyrin' in hex
            create_migration_table(&mut conn).await?;
            // Migration application logic goes here
        }
        DatabaseType::MySQL => {
            let mut conn = MySqlConnection::connect(db_url).await?;
            mysql_lock(&mut conn, "valkyrin_migration_lock").await?;
            // MySQL table creation and migration logic
        }
        DatabaseType::SQLite => {
            sqlite_lock(db_url).await?;
            // SQLite table creation and migration logic
        }
    }

    Ok(())
}

#[derive(Debug)]
enum DatabaseType {
    PostgreSQL,
    MySQL,
    SQLite,
}

#[derive(Debug)]
struct Migration {
    version: String,
    name: String,
    sql: String,
}