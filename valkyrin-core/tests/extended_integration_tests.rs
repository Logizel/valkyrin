//! Extended integration tests covering foreign‑key constraints, enum round‑trip, migration apply & rollback, and SQLite support.

use anyhow::Result;
use sqlx::AnyPool;
use std::fs;
use std::env;
use uuid::Uuid;
use valkyrin_core::sync::{SyncEngine, DatabaseType, SyncMode};
use valkyrin_core::canvas::{CanvasPayload, CanvasTable, CanvasColumn, NodePosition};

/// Helper to create a temporary working directory for each DB test.
async fn run_db_test(db_url: &str) -> Result<()> {
    // Determine DB type.
    let db_type = DatabaseType::from_url(db_url)?;
    let db_str = match db_type {
        DatabaseType::PostgreSQL => "postgres",
        DatabaseType::MySQL => "mysql",
        DatabaseType::SQLite => "sqlite",
    };

    // Create isolated temp directory.
    let temp_dir = std::env::temp_dir().join(format!("valkyrin_integ_{}", Uuid::new_v4()));
    fs::create_dir_all(&temp_dir)?;
    env::set_current_dir(&temp_dir)?;

    // Ensure a clean DB state.
    let cleanup = [
        "DROP TABLE IF EXISTS rollback_test;",
        "DROP TABLE IF EXISTS enum_test;",
        "DROP TABLE IF EXISTS posts;",
        "DROP TABLE IF EXISTS users;",
    ];
    let _ = SyncEngine::execute_migration(db_url, db_type, &cleanup.iter().map(|s| s.to_string()).collect::<Vec<_>>()).await;

    // ---------------------------------------------------------------------
    // 1. Create tables with a foreign key and an enum column.
    // ---------------------------------------------------------------------
    let enum_decl = match db_type {
        DatabaseType::PostgreSQL => "VARCHAR(255)",
        DatabaseType::MySQL => "ENUM('active','inactive')",
        DatabaseType::SQLite => "TEXT",
    };
    let enum_table_stmt = format!("CREATE TABLE enum_test (id INTEGER PRIMARY KEY, status {} NOT NULL);", enum_decl);
    let stmts = [
        "CREATE TABLE users (id INTEGER PRIMARY KEY, name VARCHAR(255) NOT NULL);",
        "CREATE TABLE posts (id INTEGER PRIMARY KEY, user_id INTEGER NOT NULL, title VARCHAR(255), CONSTRAINT fk_user FOREIGN KEY (user_id) REFERENCES users(id));",
        enum_table_stmt.as_str(),
    ];
    SyncEngine::execute_migration(db_url, db_type, &stmts.iter().map(|s| s.to_string()).collect::<Vec<_>>()).await?;

    // Verify foreign‑key enforcement.
    let pool = AnyPool::connect(db_url).await?;
    // Insert a valid user.
    sqlx::query("INSERT INTO users (id, name) VALUES (1, 'Alice');")
        .execute(&pool)
        .await?;
    // Valid post.
    sqlx::query("INSERT INTO posts (id, user_id, title) VALUES (1, 1, 'First');")
        .execute(&pool)
        .await?;
    // Invalid post – should violate FK.
    let invalid = sqlx::query("INSERT INTO posts (id, user_id, title) VALUES (2, 999, 'Invalid');")
        .execute(&pool)
        .await;
    assert!(invalid.is_err(), "Foreign‑key constraint not enforced for {}", db_str);

    // Insert enum values.
    let enum_insert = match db_type {
        DatabaseType::MySQL => "INSERT INTO enum_test (id, status) VALUES (1, 'active'), (2, 'inactive');",
        _ => "INSERT INTO enum_test (id, status) VALUES (1, 'active'), (2, 'inactive');",
    };
    sqlx::query(enum_insert).execute(&pool).await?;

    // ---------------------------------------------------------------------
    // 2. Pull live schema into the canvas (sync).
    // ---------------------------------------------------------------------
    // Ensure we start with an empty blueprint.
    fs::write("schema.vdb.json", r#"{"tables":[],"relations":[]}"#)?;
    SyncEngine::synchronize_database(db_url, Some(db_str), SyncMode::ApplyNew).await?;

    // Verify canvas contains expected tables and relation.
    let canvas_str = fs::read_to_string("schema.vdb.json")?;
    assert!(canvas_str.contains("users"), "Canvas missing users table for {}", db_str);
    assert!(canvas_str.contains("posts"), "Canvas missing posts table for {}", db_str);
    assert!(canvas_str.contains("enum_test"), "Canvas missing enum_test table for {}", db_str);
    // Detected FK should be stored as a relation entry.
    assert!(canvas_str.contains("\"relation_type\": \"1:N\""), "Canvas missing detected relation for {}", db_str);

    // Verify enum column is recognized as enum type in canvas.
    let canvas_payload: CanvasPayload = serde_json::from_str(&canvas_str)?;
    let enum_table = canvas_payload.tables.iter().find(|t| t.name == "enum_test").expect("enum_test missing");
    let enum_column = enum_table.columns.iter().find(|c| c.name == "status").expect("status column missing");
    assert_eq!(enum_column.raw_type, "enum", "Enum column not recognized in canvas for {}", db_str);

    // ---------------------------------------------------------------------
    // 3. Add a new table via canvas, generate migration, apply and rollback.
    // ---------------------------------------------------------------------
    // Load the existing payload.
    let mut payload: CanvasPayload = serde_json::from_str(&canvas_str)?;
    // Append a new table "rollback_test".
    let new_table = CanvasTable {
        id: Uuid::new_v4().to_string(),
        name: "rollback_test".to_string(),
        columns: vec![CanvasColumn {
            id: Uuid::new_v4().to_string(),
            name: "id".to_string(),
            raw_type: "int".to_string(),
            is_primary: true,
            is_nullable: false,
            is_unique: false,
            is_indexed: false,
            default_value: None,
            enum_values: None,
            precision: None,
            scale: None,
            max_length: None,
        }],
        position: NodePosition { x: 0.0, y: 0.0 },
    };
    payload.tables.push(new_table);
    fs::write("schema.vdb.json", serde_json::to_string_pretty(&payload)?)?;

    // Sync again to generate a migration for the new table.
    SyncEngine::synchronize_database(db_url, Some(db_str), SyncMode::ApplyNew).await?;

    // Run the generated migration (up).
    SyncEngine::run_migrations(db_url, Some(db_str), None).await?;

    // Verify the new table exists.
    let exists = match db_type {
        DatabaseType::PostgreSQL => sqlx::query("SELECT COUNT(*) FROM rollback_test;")
            .fetch_one(&pool)
            .await,
        DatabaseType::MySQL => sqlx::query("SELECT COUNT(*) FROM rollback_test;")
            .fetch_one(&pool)
            .await,
        DatabaseType::SQLite => sqlx::query("SELECT COUNT(*) FROM rollback_test;")
            .fetch_one(&pool)
            .await,
    };
    assert!(exists.is_ok(), "Rollback table not created for {}", db_str);

    // Roll back the last migration.
    SyncEngine::rollback_migrations(db_url, Some(db_str), 1, false).await?;

    // Verify the table has been dropped.
    let dropped = match db_type {
        DatabaseType::PostgreSQL => sqlx::query("SELECT COUNT(*) FROM rollback_test;")
            .fetch_one(&pool)
            .await,
        DatabaseType::MySQL => sqlx::query("SELECT COUNT(*) FROM rollback_test;")
            .fetch_one(&pool)
            .await,
        DatabaseType::SQLite => sqlx::query("SELECT COUNT(*) FROM rollback_test;")
            .fetch_one(&pool)
            .await,
    };
    assert!(dropped.is_err(), "Rollback table still present for {}", db_str);

    // Clean up temporary directory.
    env::set_current_dir(env::current_dir()?.parent().unwrap())?;
    fs::remove_dir_all(&temp_dir)?;
    Ok(())
}

#[tokio::test]
async fn integration_extended() -> Result<()> {
    let urls = vec![
        env::var("TEST_POSTGRES_URL").ok(),
        env::var("TEST_MYSQL_URL").ok(),
        env::var("TEST_SQLITE_URL").ok(),
    ];
    for url in urls.into_iter().flatten() {
        run_db_test(&url).await?;
    }
    Ok(())
}
