// valkyrin-core/src/sync.rs
use crate::ir::{DataType, Entity, Field};
use anyhow::{Context, Result as AnyhowResult};
use sqlx::migrate::MigrateDatabase;
use sqlx::Row;
use std::fs;

/// Supported database types for introspection
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum DatabaseType {
    PostgreSQL,
    MySQL,
    SQLite,
}

impl DatabaseType {
    /// Detect database type from connection URL
    pub fn from_url(url: &str) -> AnyhowResult<Self> {
        if url.starts_with("postgresql://") || url.starts_with("postgres://") {
            Ok(DatabaseType::PostgreSQL)
        } else if url.starts_with("mysql://") {
            Ok(DatabaseType::MySQL)
        } else if url.starts_with("sqlite://") {
            Ok(DatabaseType::SQLite)
        } else {
            Err(anyhow::anyhow!(
                "Unsupported database URL scheme. Supported: postgresql://, mysql://, sqlite://"
            ))
        }
    }
}

/// The universal contract for reading a live database.
pub trait DatabaseIntrospector {
    /// Introspects the database and returns all tables and columns.
    /// Implementation varies by database.
    fn fetch_schema(&self) -> AnyhowResult<Vec<Entity>>;
}

/// PostgreSQL introspector using information_schema
pub struct PostgresIntrospector {
    pool: sqlx::Pool<sqlx::Postgres>,
}

impl PostgresIntrospector {
    pub async fn new(url: &str) -> AnyhowResult<Self> {
        let pool = sqlx::Pool::<sqlx::Postgres>::connect(url).await.context(
            "Failed to connect to PostgreSQL. Is the URL correct?",
        )?;
        Ok(Self { pool })
    }
}

impl DatabaseIntrospector for PostgresIntrospector {
    fn fetch_schema(&self) -> AnyhowResult<Vec<Entity>> {
        // This would need to be async, but for now we'll use a blocking approach
        // In production, this should be refactored to be async throughout
        Err(anyhow::anyhow!(
            "PostgreSQL introspection requires async context. Use fetch_schema_async instead."
        ))
    }
}

impl PostgresIntrospector {
    pub async fn fetch_schema_async(&self) -> AnyhowResult<Vec<Entity>> {
        // Query the internal PostgreSQL catalog for all tables and columns
        let query = r#"
            SELECT 
                table_name, 
                column_name, 
                data_type, 
                is_nullable,
                column_default
            FROM information_schema.columns 
            WHERE table_schema = 'public'
            ORDER BY table_name, ordinal_position;
        "#;

        let rows = sqlx::query(query).fetch_all(&self.pool).await?;
        let mut entities: Vec<Entity> = Vec::new();
        let mut current_table_name = String::new();
        let mut current_fields: Vec<Field> = Vec::new();

        // First, fetch primary key information
        let pk_query = r#"
            SELECT table_name, column_name
            FROM information_schema.table_constraints tc 
            JOIN information_schema.key_column_usage kcu 
              ON tc.constraint_name = kcu.constraint_name
            WHERE tc.table_schema = 'public' AND tc.constraint_type = 'PRIMARY KEY'
        "#;
        let pk_rows = sqlx::query(pk_query).fetch_all(&self.pool).await?;
        let mut primary_keys: std::collections::HashMap<String, Vec<String>> =
            std::collections::HashMap::new();
        for row in pk_rows {
            let table_name: String = row.get("table_name");
            let column_name: String = row.get("column_name");
            primary_keys
                .entry(table_name)
                .or_insert_with(Vec::new)
                .push(column_name);
        }

        for row in rows {
            let table_name: String = row.get("table_name");
            let column_name: String = row.get("column_name");
            let db_type: String = row.get("data_type");
            let is_nullable_str: String = row.get("is_nullable");
            let column_default: Option<String> = row.get("column_default");

            // Push the previous table into our IR memory map when the table name changes
            if table_name != current_table_name && !current_table_name.is_empty() {
                entities.push(Entity {
                    id: uuid::Uuid::new_v4().to_string(),
                    name: current_table_name.clone(),
                    fields: current_fields.clone(),
                });
                current_fields.clear();
            }

            current_table_name = table_name.clone();

            // Map PostgreSQL physical types back to Valkyrin universal IR types
            let mapped_type = match db_type.as_str() {
                "character varying" | "text" => DataType::String { max_length: None },
                "integer" | "bigint" | "smallint" => DataType::Integer(crate::ir::IntSize::Standard),
                "boolean" => DataType::Boolean,
                "timestamp without time zone" | "timestamp with time zone" => DataType::DateTime,
                "jsonb" | "json" => DataType::Json,
                "numeric" | "decimal" => DataType::Decimal {
                    precision: 10,
                    scale: 2,
                },
                "real" | "double precision" => DataType::Float,
                "uuid" => DataType::Uuid,
                "bytea" => DataType::Text,
                _ => DataType::Text, // Safe fallback
            };

            let is_pk = primary_keys
                .get(&table_name)
                .map(|pks| pks.contains(&column_name))
                .unwrap_or(false);

            current_fields.push(Field {
                id: uuid::Uuid::new_v4().to_string(),
                name: column_name,
                data_type: mapped_type,
                constraints: crate::ir::Constraints {
                    is_primary_key: is_pk,
                    is_unique: false,
                    is_nullable: is_nullable_str == "YES",
                    is_indexed: false,
                    default_value: column_default,
                },
            });
        }

        // Push the final table
        if !current_table_name.is_empty() {
            entities.push(Entity {
                id: uuid::Uuid::new_v4().to_string(),
                name: current_table_name,
                fields: current_fields,
            });
        }

        Ok(entities)
    }
}

/// MySQL introspector using information_schema
pub struct MysqlIntrospector {
    pool: sqlx::Pool<sqlx::MySql>,
}

impl MysqlIntrospector {
    pub async fn new(url: &str) -> AnyhowResult<Self> {
        let pool = sqlx::Pool::<sqlx::MySql>::connect(url)
            .await
            .context("Failed to connect to MySQL. Is the URL correct?")?;
        Ok(Self { pool })
    }
}

impl DatabaseIntrospector for MysqlIntrospector {
    fn fetch_schema(&self) -> AnyhowResult<Vec<Entity>> {
        Err(anyhow::anyhow!(
            "MySQL introspection requires async context. Use fetch_schema_async instead."
        ))
    }
}

impl MysqlIntrospector {
    pub async fn fetch_schema_async(&self) -> AnyhowResult<Vec<Entity>> {
        // Query MySQL information_schema for tables and columns
        let query = r#"
            SELECT 
                TABLE_NAME,
                COLUMN_NAME,
                COLUMN_TYPE,
                IS_NULLABLE,
                COLUMN_DEFAULT
            FROM INFORMATION_SCHEMA.COLUMNS
            WHERE TABLE_SCHEMA = DATABASE()
            ORDER BY TABLE_NAME, ORDINAL_POSITION
        "#;

        let rows = sqlx::query(query).fetch_all(&self.pool).await?;
        let mut entities: Vec<Entity> = Vec::new();
        let mut current_table_name = String::new();
        let mut current_fields: Vec<Field> = Vec::new();

        // Fetch primary keys
        let pk_query = r#"
            SELECT TABLE_NAME, COLUMN_NAME
            FROM INFORMATION_SCHEMA.KEY_COLUMN_USAGE
            WHERE TABLE_SCHEMA = DATABASE() AND CONSTRAINT_NAME = 'PRIMARY'
        "#;
        let pk_rows = sqlx::query(pk_query).fetch_all(&self.pool).await?;
        let mut primary_keys: std::collections::HashMap<String, Vec<String>> =
            std::collections::HashMap::new();
        for row in pk_rows {
            let table_name: String = row.get("TABLE_NAME");
            let column_name: String = row.get("COLUMN_NAME");
            primary_keys
                .entry(table_name)
                .or_insert_with(Vec::new)
                .push(column_name);
        }

        for row in rows {
            let table_name: String = row.get("TABLE_NAME");
            let column_name: String = row.get("COLUMN_NAME");
            let column_type: String = row.get("COLUMN_TYPE");
            let is_nullable_str: String = row.get("IS_NULLABLE");
            let column_default: Option<String> = row.get("COLUMN_DEFAULT");

            if table_name != current_table_name && !current_table_name.is_empty() {
                entities.push(Entity {
                    id: uuid::Uuid::new_v4().to_string(),
                    name: current_table_name.clone(),
                    fields: current_fields.clone(),
                });
                current_fields.clear();
            }

            current_table_name = table_name.clone();

            // Map MySQL types to Valkyrin types
            let mapped_type = match column_type.as_str() {
                s if s.starts_with("varchar") || s.starts_with("char") => {
                    DataType::String { max_length: None }
                }
                "text" | "longtext" | "mediumtext" | "tinytext" => DataType::Text,
                "tinyint" | "smallint" | "int" | "integer" | "mediumint" => {
                    DataType::Integer(crate::ir::IntSize::Standard)
                }
                "bigint" => DataType::Integer(crate::ir::IntSize::Big),
                "float" | "double" | "real" => DataType::Float,
                "decimal" | "numeric" => DataType::Decimal {
                    precision: 10,
                    scale: 2,
                },
                "boolean" | "bool" => DataType::Boolean,
                "timestamp" | "datetime" | "date" | "time" => DataType::DateTime,
                "json" => DataType::Json,
                "uuid" => DataType::Uuid,
                _ => DataType::Text,
            };

            let is_pk = primary_keys
                .get(&table_name)
                .map(|pks| pks.contains(&column_name))
                .unwrap_or(false);

            current_fields.push(Field {
                id: uuid::Uuid::new_v4().to_string(),
                name: column_name,
                data_type: mapped_type,
                constraints: crate::ir::Constraints {
                    is_primary_key: is_pk,
                    is_unique: false,
                    is_nullable: is_nullable_str == "YES",
                    is_indexed: false,
                    default_value: column_default,
                },
            });
        }

        if !current_table_name.is_empty() {
            entities.push(Entity {
                id: uuid::Uuid::new_v4().to_string(),
                name: current_table_name,
                fields: current_fields,
            });
        }

        Ok(entities)
    }
}

/// SQLite introspector
pub struct SqliteIntrospector {
    pool: sqlx::Pool<sqlx::Sqlite>,
}

impl SqliteIntrospector {
    pub async fn new(url: &str) -> AnyhowResult<Self> {
        // Ensure the database exists (SQLite creates it if it doesn't)
        if !sqlx::Sqlite::database_exists(url).await.unwrap_or(false) {
            sqlx::Sqlite::create_database(url).await?;
        }
        let pool = sqlx::Pool::<sqlx::Sqlite>::connect(url)
            .await
            .context("Failed to connect to SQLite. Is the URL correct?")?;
        Ok(Self { pool })
    }
}

impl DatabaseIntrospector for SqliteIntrospector {
    fn fetch_schema(&self) -> AnyhowResult<Vec<Entity>> {
        Err(anyhow::anyhow!(
            "SQLite introspection requires async context. Use fetch_schema_async instead."
        ))
    }
}

impl SqliteIntrospector {
    pub async fn fetch_schema_async(&self) -> AnyhowResult<Vec<Entity>> {
        // Get all tables except internal SQLite tables
        let tables_query = r#"SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%';"#;
        let table_rows = sqlx::query(tables_query).fetch_all(&self.pool).await?;

        let mut entities: Vec<Entity> = Vec::new();

        for table_row in table_rows {
            let table_name: String = table_row.get("name");

            // Get columns for this table
            let columns_query = format!("PRAGMA table_info({});", table_name);
            let column_rows = sqlx::query(&columns_query).fetch_all(&self.pool).await?;

            let mut fields: Vec<Field> = Vec::new();

            for col_row in column_rows {
                let column_name: String = col_row.get("name");
                let sql_type: String = col_row.get("type");
                let is_nullable: i32 = col_row.get("notnull");
                let pk_info: i32 = col_row.get("pk");
                let default_value: Option<String> = col_row.get("dflt_value");

                // Map SQLite types to Valkyrin types
                let mapped_type = match sql_type.to_lowercase().as_str() {
                    s if s.contains("varchar") || s.contains("text") => {
                        if s.contains("char") {
                            DataType::String { max_length: None }
                        } else {
                            DataType::Text
                        }
                    }
                    s if s.contains("int") => DataType::Integer(crate::ir::IntSize::Standard),
                    s if s.contains("real") || s.contains("float") || s.contains("double") => {
                        DataType::Float
                    }
                    s if s.contains("numeric") || s.contains("decimal") => DataType::Decimal {
                        precision: 10,
                        scale: 2,
                    },
                    s if s.contains("bool") => DataType::Boolean,
                    s if s.contains("date") || s.contains("time") => DataType::DateTime,
                    s if s.contains("json") => DataType::Json,
                    s if s.contains("uuid") || s.contains("guid") => DataType::Uuid,
                    _ => DataType::Text,
                };

                fields.push(Field {
                    id: uuid::Uuid::new_v4().to_string(),
                    name: column_name,
                    data_type: mapped_type,
                    constraints: crate::ir::Constraints {
                        is_primary_key: pk_info > 0,
                        is_unique: false,
                        is_nullable: is_nullable == 0,
                        is_indexed: false,
                        default_value,
                    },
                });
            }

            entities.push(Entity {
                id: uuid::Uuid::new_v4().to_string(),
                name: table_name,
                fields,
            });
        }

        Ok(entities)
    }
}

pub struct SyncEngine;

pub struct SchemaDiff {
    pub new_tables: Vec<Entity>,
    pub removed_tables: Vec<String>,
}

impl SyncEngine {
    /// Compares the live database state against the local canvas state.
    pub fn calculate_diff(live_schema: &[Entity], local_schema: &[Entity]) -> SchemaDiff {
        let mut diff = SchemaDiff {
            new_tables: Vec::new(),
            removed_tables: Vec::new(),
        };

        // Find new tables
        for live_table in live_schema {
            let found_locally = local_schema.iter().any(|loc| loc.name == live_table.name);
            if !found_locally {
                diff.new_tables.push(live_table.clone());
            }
        }

        // Find removed tables
        for local_table in local_schema {
            let found_live = live_schema.iter().any(|live| live.name == local_table.name);
            if !found_live {
                diff.removed_tables.push(local_table.name.clone());
            }
        }

        diff
    }

    /// Calculates a safe X/Y coordinate for a new table so it does not overlap existing tables.
    pub fn calculate_safe_spawn_point(existing_layout: &[(f32, f32)]) -> (f32, f32) {
        if existing_layout.is_empty() {
            return (100.0, 100.0);
        }

        let mut max_x = 0.0;
        let mut base_y = 100.0;

        for (x, y) in existing_layout {
            if *x > max_x {
                max_x = *x;
                base_y = *y;
            }
        }

        (max_x + 300.0, base_y)
    }

    /// Connects to a database (auto-detects type from URL), diffs the live schema against
    /// the local canvas, and updates the JSON layout.
    pub async fn synchronize_database(db_url: &str) -> AnyhowResult<()> {
        println!("🔌 Connecting to database...");

        // Auto-detect database type from URL
        let db_type = DatabaseType::from_url(db_url)?;

        let live_schema = match db_type {
            DatabaseType::PostgreSQL => {
                let introspector = PostgresIntrospector::new(db_url).await?;
                println!("🔍 Introspecting live PostgreSQL schema...");
                introspector.fetch_schema_async().await?
            }
            DatabaseType::MySQL => {
                let introspector = MysqlIntrospector::new(db_url).await?;
                println!("🔍 Introspecting live MySQL schema...");
                introspector.fetch_schema_async().await?
            }
            DatabaseType::SQLite => {
                let introspector = SqliteIntrospector::new(db_url).await?;
                println!("🔍 Introspecting live SQLite schema...");
                introspector.fetch_schema_async().await?
            }
        };

        // Load the local canvas layout
        let local_file = fs::read_to_string("schema.vdb.json")
            .unwrap_or_else(|_| r#"{"tables":[],"relations":[]}"#.to_string());
        let mut payload: crate::canvas::CanvasPayload =
            serde_json::from_str(&local_file).context("Failed to parse local schema.vdb.json")?;

        // Diff the schemas
        let local_ir = payload.to_ir();
        let diff = Self::calculate_diff(&live_schema, &local_ir.entities);

        if diff.new_tables.is_empty() && diff.removed_tables.is_empty() {
            println!("✅ Canvas is already perfectly synced with the live database.");
            return Ok(());
        }

        if !diff.removed_tables.is_empty() {
            println!(
                "⚠️  {} table(s) removed from database: {}",
                diff.removed_tables.len(),
                diff.removed_tables.join(", ")
            );
            println!("   (Use --confirm to delete them from canvas)");
        }

        // Safely inject new tables into the visual layout
        let existing_positions: Vec<(f32, f32)> = payload
            .tables
            .iter()
            .map(|t| (t.position.x, t.position.y))
            .collect();

        let mut next_spawn = Self::calculate_safe_spawn_point(&existing_positions);

        for new_table in diff.new_tables {
            println!(
                "✨ Discovered missing table in production: {}",
                new_table.name
            );

            let mut canvas_columns = Vec::new();
            for field in new_table.fields {
                let type_str = match field.data_type {
                    DataType::String { .. } => "string",
                    DataType::Text => "text",
                    DataType::Integer(crate::ir::IntSize::Small) => "smallint",
                    DataType::Integer(crate::ir::IntSize::Standard) => "int",
                    DataType::Integer(crate::ir::IntSize::Big) => "bigint",
                    DataType::Float => "float",
                    DataType::Decimal { .. } => "decimal",
                    DataType::Boolean => "boolean",
                    DataType::DateTime => "datetime",
                    DataType::Json => "json",
                    DataType::Uuid => "uuid",
                    DataType::Enum(_) => "string",
                };

                canvas_columns.push(crate::canvas::CanvasColumn {
                    id: field.id,
                    name: field.name,
                    raw_type: type_str.to_string(),
                    is_primary: field.constraints.is_primary_key,
                    is_nullable: field.constraints.is_nullable,
                    is_unique: field.constraints.is_unique,
                    is_indexed: field.constraints.is_indexed,
                    default_value: field.constraints.default_value,
                });
            }

            payload.tables.push(crate::canvas::CanvasTable {
                id: new_table.id,
                name: new_table.name,
                columns: canvas_columns,
                position: crate::canvas::NodePosition {
                    x: next_spawn.0,
                    y: next_spawn.1,
                },
            });

            next_spawn.1 += 250.0;
        }

        // Write the updated blueprint back to disk
        let pretty_json = serde_json::to_string_pretty(&payload)?;
        fs::write("schema.vdb.json", pretty_json)?;
        println!("💾 Canvas blueprint updated! Boot the canvas to see the new tables.");

        Ok(())
    }
}
