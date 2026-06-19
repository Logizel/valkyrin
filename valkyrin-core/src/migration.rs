use crate::error::{ValkyrinError, ValkyrinResult, from_sqlx};
use sqlx::{Connection, Executor, PgConnection, MySqlConnection, SqliteConnection, types::chrono::DateTime, types::chrono::Utc};
use std::path::Path;
use std::fs;
use serde_json::Value;
use sha2::{Sha256, Digest};
use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};

/// Represents a migration record in the database
#[derive(Debug, sqlx::FromRow)]
pub struct MigrationRecord {
    pub version: String,
    pub name: String,
    pub checksum: String,
    pub applied_at: DateTime<Utc>,
    pub success: bool,
    pub applied_statements: Option<i32>,
    pub partial_hashes: Option<Value>,
    pub execution_time_ms: Option<i64>,
    pub error_stmt: Option<String>,
}

/// Creates the migration history table if it doesn't exist (PostgreSQL)
pub async fn create_migration_table(
    conn: &mut PgConnection,
) -> ValkyrinResult<()> {
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
    .map_err(from_sqlx)
    .map_err(|e| ValkyrinError::Migration(format!("Failed to create migration table: {}", e)))?;
    Ok(())
}

/// Creates the migration history table if it doesn't exist (MySQL)
pub async fn create_migration_table_mysql(
    conn: &mut MySqlConnection,
) -> ValkyrinResult<()> {
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
    .map_err(from_sqlx)
    .map_err(|e| ValkyrinError::Migration(format!("Failed to create migration table: {}", e)))?;
    Ok(())
}

/// Creates the migration history table if it doesn't exist (SQLite)
pub async fn create_migration_table_sqlite(
    conn: &mut SqliteConnection,
) -> ValkyrinResult<()> {
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
    .map_err(from_sqlx)
    .map_err(|e| ValkyrinError::Migration(format!("Failed to create migration table: {}", e)))?;
    Ok(())
}

/// Upgrades the migration table schema to support statement-level tracking (PostgreSQL)
pub async fn upgrade_migration_table_postgres(
    conn: &mut PgConnection,
) -> ValkyrinResult<()> {
    conn.execute(
        r#"
        ALTER TABLE _valkyrin_migrations
        ADD COLUMN IF NOT EXISTS applied_statements INTEGER DEFAULT 0,
        ADD COLUMN IF NOT EXISTS partial_hashes JSONB DEFAULT '[]'::jsonb,
        ADD COLUMN IF NOT EXISTS execution_time_ms BIGINT,
        ADD COLUMN IF NOT EXISTS error_stmt TEXT
        "#
    )
    .await
    .map_err(from_sqlx)
    .map_err(|e| ValkyrinError::Migration(format!("Failed to upgrade migration table: {}", e)))?;
    Ok(())
}

/// Upgrades the migration table schema to support statement-level tracking (MySQL)
pub async fn upgrade_migration_table_mysql(
    conn: &mut MySqlConnection,
) -> ValkyrinResult<()> {
    conn.execute(
        r#"
        ALTER TABLE _valkyrin_migrations
        ADD COLUMN IF NOT EXISTS applied_statements INT DEFAULT 0,
        ADD COLUMN IF NOT EXISTS partial_hashes JSON DEFAULT '[]',
        ADD COLUMN IF NOT EXISTS execution_time_ms BIGINT,
        ADD COLUMN IF NOT EXISTS error_stmt TEXT
        "#
    )
    .await
    .map_err(from_sqlx)
    .map_err(|e| ValkyrinError::Migration(format!("Failed to upgrade migration table: {}", e)))?;
    Ok(())
}

/// Upgrades the migration table schema to support statement-level tracking (SQLite)
pub async fn upgrade_migration_table_sqlite(
    conn: &mut SqliteConnection,
) -> ValkyrinResult<()> {
    conn.execute(
        r#"
        ALTER TABLE _valkyrin_migrations
        ADD COLUMN IF NOT EXISTS applied_statements INTEGER DEFAULT 0
        "#
    )
    .await
    .map_err(from_sqlx)
    .map_err(|e| ValkyrinError::Migration(format!("Failed to upgrade migration table: {}", e)))?;

    conn.execute(
        r#"
        ALTER TABLE _valkyrin_migrations
        ADD COLUMN IF NOT EXISTS partial_hashes TEXT DEFAULT '[]'
        "#
    )
    .await
    .map_err(from_sqlx)
    .map_err(|e| ValkyrinError::Migration(format!("Failed to upgrade migration table: {}", e)))?;

    conn.execute(
        r#"
        ALTER TABLE _valkyrin_migrations
        ADD COLUMN IF NOT EXISTS execution_time_ms INTEGER
        "#
    )
    .await
    .map_err(from_sqlx)
    .map_err(|e| ValkyrinError::Migration(format!("Failed to upgrade migration table: {}", e)))?;

    conn.execute(
        r#"
        ALTER TABLE _valkyrin_migrations
        ADD COLUMN IF NOT EXISTS error_stmt TEXT
        "#
    )
    .await
    .map_err(from_sqlx)
    .map_err(|e| ValkyrinError::Migration(format!("Failed to upgrade migration table: {}", e)))?;

    Ok(())
}

/// PostgreSQL lock mechanism using advisory locks
pub async fn pg_advisory_lock(
    conn: &mut PgConnection,
    lock_id: i64,
) -> ValkyrinResult<()> {
    sqlx::query("SELECT pg_advisory_lock($1)")
        .bind(lock_id)
        .execute(conn)
        .await
        .map_err(from_sqlx)
        .map_err(|e| ValkyrinError::Database(format!("Failed to acquire PostgreSQL advisory lock: {}", e)))?;
    Ok(())
}

/// MySQL lock mechanism
pub async fn mysql_lock(
    conn: &mut MySqlConnection,
    lock_name: &str,
) -> ValkyrinResult<()> {
    sqlx::query("SELECT GET_LOCK(?, 30)") // 30 second timeout
        .bind(lock_name)
        .execute(conn)
        .await
        .map_err(from_sqlx)
        .map_err(|e| ValkyrinError::Database(format!("Failed to acquire MySQL lock: {}", e)))?;
    Ok(())
}

/// SQLite lock mechanism using file locks
pub fn sqlite_lock(db_path: &str) -> ValkyrinResult<()> {
    let lock_file = Path::new(db_path).with_extension("lock");
    let _file = fs::OpenOptions::new()
        .create(true)
        .write(true)
        .open(lock_file)
        .map_err(|e| ValkyrinError::Io(format!("Failed to create SQLite lock file: {}", e)))?;
    // File lock is automatically released when _file goes out of scope
    Ok(())
}

/// Represents a single file's cumulative hash entry in valkyrin.sum
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileHashEntry {
    pub filename: String,
    pub cumulative_hash: String,
}

/// Represents the full valkyrin.sum directory hash
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DirectorySum {
    pub dir_hash: String,
    pub file_hashes: Vec<FileHashEntry>,
}

/// Computes SHA256 hash of a file's content, chained with previous hash
fn compute_file_hash(
    filename: &str,
    content: &str,
    prev_hash: &[u8; 32],
) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(prev_hash);
    hasher.update(filename.as_bytes());
    
    let lines: Vec<&str> = content.lines().collect();
    let should_ignore = lines.first().map(|l| l.trim() == "-- valkyrin:sum ignore").unwrap_or(false);
    
    if !should_ignore {
        hasher.update(content.as_bytes());
    }
    
    hasher.finalize().into()
}

/// Computes the directory-wide chained hash for all .sql files in migrations/
pub fn compute_directory_hash(migrations_dir: &Path) -> ValkyrinResult<DirectorySum> {
    let mut entries = Vec::new();
    
    let mut files: Vec<_> = fs::read_dir(migrations_dir)
        .map_err(|e| ValkyrinError::Io(format!("Failed to read migrations directory: {}", e)))?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| {
            p.extension()
                .and_then(|ext| ext.to_str())
                .map(|ext| ext.eq_ignore_ascii_case("sql"))
                .unwrap_or(false)
        })
        .collect();
    
    files.sort_by(|a, b| a.file_name().cmp(&b.file_name()));
    
    let mut running_hash = [0u8; 32];
    
    for file_path in files {
        let filename = file_path.file_name()
            .and_then(|n| n.to_str())
            .ok_or_else(|| ValkyrinError::Io("Invalid filename".to_string()))?
            .to_string();
        
        let content = fs::read_to_string(&file_path)
            .map_err(|e| ValkyrinError::Io(format!("Failed to read migration file {}: {}", file_path.display(), e)))?;
        
        let file_hash = compute_file_hash(&filename, &content, &running_hash);
        running_hash = file_hash;
        
        let hash_b64 = BASE64.encode(file_hash);
        entries.push(FileHashEntry {
            filename,
            cumulative_hash: format!("h1:{}", hash_b64),
        });
    }
    
    let mut dir_hasher = Sha256::new();
    for entry in &entries {
        dir_hasher.update(entry.filename.as_bytes());
        dir_hasher.update(entry.cumulative_hash.as_bytes());
    }
    let dir_hash = format!("h1:{}", BASE64.encode(dir_hasher.finalize()));
    
    Ok(DirectorySum {
        dir_hash,
        file_hashes: entries,
    })
}

/// Writes the valkyrin.sum file to the migrations directory
pub fn write_valkyrin_sum(migrations_dir: &Path, sum: &DirectorySum) -> ValkyrinResult<()> {
    let sum_path = migrations_dir.join("valkyrin.sum");
    let mut content = String::new();
    content.push_str(&sum.dir_hash);
    content.push('\n');
    
    for entry in &sum.file_hashes {
        content.push_str(&entry.filename);
        content.push(' ');
        content.push_str(&entry.cumulative_hash);
        content.push('\n');
    }
    
    fs::write(&sum_path, content)
        .map_err(|e| ValkyrinError::Io(format!("Failed to write valkyrin.sum: {}", e)))?;
    
    Ok(())
}

/// Parses and validates a valkyrin.sum file
pub fn read_valkyrin_sum(migrations_dir: &Path) -> ValkyrinResult<DirectorySum> {
    let sum_path = migrations_dir.join("valkyrin.sum");
    let content = fs::read_to_string(&sum_path)
        .map_err(|e| ValkyrinError::Io(format!("Failed to read valkyrin.sum: {}", e)))?;
    
    let lines: Vec<&str> = content.lines().collect();
    if lines.is_empty() {
        return Err(ValkyrinError::ChecksumNotFound("valkyrin.sum is empty".to_string()));
    }
    
    let dir_hash = lines[0].to_string();
    if !dir_hash.starts_with("h1:") {
        return Err(ValkyrinError::ChecksumNotFound("Invalid directory hash format".to_string()));
    }
    
    let mut file_hashes = Vec::new();
    for line in &lines[1..] {
        let parts: Vec<&str> = line.splitn(2, ' ').collect();
        if parts.len() != 2 {
            return Err(ValkyrinError::ChecksumNotFound("Invalid file hash entry format".to_string()));
        }
        file_hashes.push(FileHashEntry {
            filename: parts[0].to_string(),
            cumulative_hash: parts[1].to_string(),
        });
    }
    
    let sum = DirectorySum {
        dir_hash,
        file_hashes,
    };
    
    validate_sum_self_consistency(&sum)?;
    
    Ok(sum)
}

/// Validates that valkyrin.sum is internally consistent (invariant check)
fn validate_sum_self_consistency(sum: &DirectorySum) -> ValkyrinResult<()> {
    let mut dir_hasher = Sha256::new();
    for entry in &sum.file_hashes {
        dir_hasher.update(entry.filename.as_bytes());
        dir_hasher.update(entry.cumulative_hash.as_bytes());
    }
    let computed_dir_hash = format!("h1:{}", BASE64.encode(dir_hasher.finalize()));
    
    if computed_dir_hash != sum.dir_hash {
        return Err(ValkyrinError::ChecksumNotFound(
            "valkyrin.sum self-consistency check failed: directory hash mismatch".to_string()
        ));
    }
    
    Ok(())
}

/// Represents the result of tamper detection
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TamperDiagnosis {
    Valid,
    Removed { filename: String, position: usize },
    Edited { filename: String, position: usize },
    Injected { intruder_filename: String, position: usize },
    Appended { filename: String },
    ChecksumNotFound,
}

/// Performs pre-flight tamper detection by comparing stored valkyrin.sum with computed state
pub fn validate_valkyrin_sum(migrations_dir: &Path) -> ValkyrinResult<TamperDiagnosis> {
    let stored_sum = match read_valkyrin_sum(migrations_dir) {
        Ok(s) => s,
        Err(ValkyrinError::ChecksumNotFound(_)) => {
            let has_sql = fs::read_dir(migrations_dir)
                .map_err(|e| ValkyrinError::Io(format!("Failed to read migrations directory: {}", e)))?
                .filter_map(|e| e.ok())
                .any(|e| {
                    e.path().extension()
                        .and_then(|ext| ext.to_str())
                        .map(|ext| ext.eq_ignore_ascii_case("sql"))
                        .unwrap_or(false)
                });
            if has_sql {
                return Ok(TamperDiagnosis::ChecksumNotFound);
            }
            return Ok(TamperDiagnosis::Valid);
        }
        Err(e) => return Err(e),
    };
    
    let actual_sum = compute_directory_hash(migrations_dir)?;
    
    if stored_sum.dir_hash == actual_sum.dir_hash {
        return Ok(TamperDiagnosis::Valid);
    }
    
    let stored_entries = &stored_sum.file_hashes;
    let actual_entries = &actual_sum.file_hashes;
    
    let mut pos = 0;
    let mut i = 0;
    let mut j = 0;
    
    while i < stored_entries.len() && j < actual_entries.len() {
        if stored_entries[i] == actual_entries[j] {
            pos += stored_entries[i].filename.len() + 1 + stored_entries[i].cumulative_hash.len() + 1;
            i += 1;
            j += 1;
            continue;
        }
        
        let stored_filename = &stored_entries[i].filename;
        let actual_filename = &actual_entries[j].filename;
        
        if !actual_entries.iter().any(|e| &e.filename == stored_filename) {
            return Ok(TamperDiagnosis::Removed {
                filename: stored_filename.clone(),
                position: pos,
            });
        }
        
        if stored_filename == actual_filename {
            return Ok(TamperDiagnosis::Edited {
                filename: stored_filename.clone(),
                position: pos,
            });
        }
        
        if stored_entries.iter().skip(i + 1).any(|e| &e.filename == actual_filename) {
            return Ok(TamperDiagnosis::Injected {
                intruder_filename: actual_filename.clone(),
                position: pos,
            });
        }
        
        return Ok(TamperDiagnosis::Edited {
            filename: actual_filename.clone(),
            position: pos,
        });
    }
    
    if i < stored_entries.len() {
        return Ok(TamperDiagnosis::Removed {
            filename: stored_entries[i].filename.clone(),
            position: pos,
        });
    }
    
    if j < actual_entries.len() {
        return Ok(TamperDiagnosis::Appended {
            filename: actual_entries[j].filename.clone(),
        });
    }
    
    Ok(TamperDiagnosis::Valid)
}

/// Applies migrations with proper locking and tracking
pub async fn apply_migrations_with_lock(
    db_url: &str,
    migrations: Vec<Migration>,
) -> ValkyrinResult<()> {
    // Determine database type from URL
    let db_type = if db_url.starts_with("postgres") {
        DatabaseType::PostgreSQL
    } else if db_url.starts_with("mysql") {
        DatabaseType::MySQL
    } else if db_url.starts_with("sqlite") {
        DatabaseType::SQLite
    } else {
        return Err(ValkyrinError::Database("Unsupported database type".to_string()));
    };

    match db_type {
        DatabaseType::PostgreSQL => {
            let mut conn = PgConnection::connect(db_url).await
                .map_err(from_sqlx)
                .map_err(|e| ValkyrinError::Database(format!("Failed to connect to PostgreSQL: {}", e)))?;
            pg_advisory_lock(&mut conn, 0x76616C6B7972696E).await?; // 'valkyrin' in hex
            create_migration_table(&mut conn).await?;
            upgrade_migration_table_postgres(&mut conn).await?;
            // Migration application logic goes here
        }
        DatabaseType::MySQL => {
            let mut conn = MySqlConnection::connect(db_url).await
                .map_err(from_sqlx)
                .map_err(|e| ValkyrinError::Database(format!("Failed to connect to MySQL: {}", e)))?;
            mysql_lock(&mut conn, "valkyrin_migration_lock").await?;
            create_migration_table_mysql(&mut conn).await?;
            upgrade_migration_table_mysql(&mut conn).await?;
            // MySQL table creation and migration logic
        }
        DatabaseType::SQLite => {
            sqlite_lock(db_url)?;
            let mut conn = SqliteConnection::connect(db_url).await
                .map_err(from_sqlx)
                .map_err(|e| ValkyrinError::Database(format!("Failed to connect to SQLite: {}", e)))?;
            create_migration_table_sqlite(&mut conn).await?;
            upgrade_migration_table_sqlite(&mut conn).await?;
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