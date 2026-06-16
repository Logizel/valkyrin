//! Integration tests for migration execution against real databases (PostgreSQL & MySQL).
//! These tests run in CI using Testcontainers services defined in .github/workflows/ci.yml.
//! They verify that the `SyncEngine::execute_migration` helper can apply a simple schema change.

use valkyrin_core::sync::{SyncEngine, DatabaseType};

#[tokio::test]
async fn integration_migration_execution() {
    // Collect DB URLs from CI environment variables.
    let db_urls = vec![
        std::env::var("TEST_POSTGRES_URL").ok(),
        std::env::var("TEST_MYSQL_URL").ok(),
    ];

    for url in db_urls.into_iter().flatten() {
            // Determine the DB type from the URL.
            let db_type = DatabaseType::from_url(&url).expect("Unsupported database URL");

            // Ensure any previous run is cleaned up.
            let _ = SyncEngine::execute_migration(
                &url,
                db_type,
                &["DROP TABLE IF EXISTS integration_test;".to_string()],
            )
            .await;

            // Simple migration: create a table with a single integer primary key.
            let create_stmt = "CREATE TABLE integration_test (id INTEGER PRIMARY KEY);";
            SyncEngine::execute_migration(&url, db_type, &[create_stmt.to_string()])
                .await
                .expect("Failed to execute migration on database");

            // Verify the table exists by performing a SELECT COUNT(*).
            match db_type {
                DatabaseType::PostgreSQL => {
                    let pool = sqlx::Pool::<sqlx::Postgres>::connect(&url).await.unwrap();
                    let row: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM integration_test")
                        .fetch_one(&pool)
                        .await
                        .unwrap();
                    assert_eq!(row.0, 0);
                }
                DatabaseType::MySQL => {
                    let pool = sqlx::Pool::<sqlx::MySql>::connect(&url).await.unwrap();
                    let row: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM integration_test")
                        .fetch_one(&pool)
                        .await
                        .unwrap();
                    assert_eq!(row.0, 0);
                }
                DatabaseType::SQLite => { /* not used in CI */ }
            }

            // Clean up the test table.
            let _ = SyncEngine::execute_migration(
                &url,
                db_type,
                &["DROP TABLE integration_test;".to_string()],
            )
            .await;
    }
}
