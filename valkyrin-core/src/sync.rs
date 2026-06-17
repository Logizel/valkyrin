// valkyrin-core/src/sync.rs
use crate::error::{ValkyrinError, ValkyrinResult, from_sqlx};
use crate::ir::{DataType, Entity, Field};
use async_trait::async_trait;
use sqlx::migrate::MigrateDatabase;
use sqlx::Row;
use std::fs;
use std::time::{SystemTime, UNIX_EPOCH};
use crate::migration::{create_migration_table, create_migration_table_mysql, create_migration_table_sqlite, pg_advisory_lock, mysql_lock, sqlite_lock, MigrationRecord};
use sha2::{Sha256, Digest};
use std::collections::HashMap;

/// Supported database types for introspection
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum DatabaseType {
    PostgreSQL,
    MySQL,
    SQLite,
}

impl DatabaseType {
    /// Detect database type from connection URL
    pub fn from_url(url: &str) -> ValkyrinResult<Self> {
        if url.starts_with("postgresql://") || url.starts_with("postgres://") {
            Ok(DatabaseType::PostgreSQL)
        } else if url.starts_with("mysql://") {
            Ok(DatabaseType::MySQL)
        } else if url.starts_with("sqlite://") {
            Ok(DatabaseType::SQLite)
        } else {
            Err(ValkyrinError::Database(
                "Unsupported database URL scheme. Supported: postgresql://, mysql://, sqlite://".to_string()
            ))
        }
    }
}

// ──────────────────────────────────────────────
// Detailed Diff Structures (column-level)
// ──────────────────────────────────────────────

/// Describes what changed about a single column
#[derive(Debug, Clone)]
pub enum ColumnChange {
    TypeChanged {
        name: String,
        from: DataType,
        to: DataType,
    },
    NullableChanged {
        name: String,
        was_nullable: bool,
        is_nullable: bool,
    },
    UniqueChanged {
        name: String,
        was_unique: bool,
        is_unique: bool,
    },
    DefaultChanged {
        name: String,
        before: Option<String>,
        after: Option<String>,
    },
    /// PostgreSQL enum type was added or values were added
    EnumTypeChanged {
        name: String,
        type_name: String,
        added_values: Vec<String>,
    },
}

/// Per-table diff: which columns were added/removed/modified
#[derive(Debug, Clone)]
pub struct TableDiff {
    pub table_name: String,
    pub adds: Vec<Field>,
    pub removes: Vec<Field>,
    pub changes: Vec<ColumnChange>,
}

/// Complete bidirectional diff report
#[derive(Debug, Clone)]
pub struct DetailedDiff {
    /// Tables that exist in the database but not on the canvas
    pub new_tables: Vec<Entity>,
    /// Tables that exist on the canvas but not in the database
    pub removed_tables: Vec<String>,
    /// Tables that exist on both sides — detailed column diff
    pub modified_tables: Vec<TableDiff>,
    /// Foreign key relationships detected in the live database
    pub detected_relations: Vec<DetectedRelation>,
}

/// A foreign key relationship detected from a live database
#[derive(Debug, Clone)]
pub struct DetectedRelation {
    pub source_table: String,
    pub source_column: String,
    pub target_table: String,
    pub target_column: String,
}

/// The flavor of the output — dry-run shows what would happen without applying
#[derive(Debug, Clone, PartialEq)]
pub enum SyncMode {
    /// Show the diff and apply new tables only
    ApplyNew,
    /// Show the diff and apply everything (including destructive changes)
    ApplyAll,
    /// Only show the diff, do not write anything
    DryRun,
}

// ──────────────────────────────────────────────
// Introspector Trait
// ──────────────────────────────────────────────

#[async_trait]
pub trait DatabaseIntrospector: Send + Sync {
    async fn fetch_schema(&self) -> ValkyrinResult<Vec<Entity>>;
    async fn fetch_relations(&self) -> ValkyrinResult<Vec<DetectedRelation>>;
}

// ──────────────────────────────────────────────
// PostgreSQL
// ──────────────────────────────────────────────

pub struct PostgresIntrospector {
    pool: sqlx::Pool<sqlx::Postgres>,
}

impl PostgresIntrospector {
    pub async fn new(url: &str) -> ValkyrinResult<Self> {
        let pool = sqlx::Pool::<sqlx::Postgres>::connect(url).await
            .map_err(from_sqlx)
            .map_err(|e| ValkyrinError::Database(format!("Failed to connect to PostgreSQL: {}", e)))?;
        Ok(Self { pool })
    }
}

#[async_trait]
impl DatabaseIntrospector for PostgresIntrospector {
    async fn fetch_schema(&self) -> ValkyrinResult<Vec<Entity>> {
        let query = r#"
            SELECT
                table_name,
                column_name,
                data_type,
                is_nullable,
                column_default,
                udt_name
            FROM information_schema.columns
            WHERE table_schema = 'public'
            ORDER BY table_name, ordinal_position;
        "#;

        let rows = sqlx::query(query)
            .fetch_all(&self.pool)
            .await
            .map_err(from_sqlx)
            .map_err(|e| ValkyrinError::Introspection(format!("Failed to fetch PostgreSQL schema: {}", e)))?;
        let mut entities: Vec<Entity> = Vec::new();
        let mut current_table_name = String::new();
        let mut current_fields: Vec<Field> = Vec::new();

        let pk_query = r#"
            SELECT table_name, column_name, ordinal_position
            FROM information_schema.table_constraints tc
            JOIN information_schema.key_column_usage kcu
              ON tc.constraint_name = kcu.constraint_name
            WHERE tc.table_schema = 'public' AND tc.constraint_type = 'PRIMARY KEY'
            ORDER BY table_name, ordinal_position
        "#;
        let pk_rows = sqlx::query(pk_query)
            .fetch_all(&self.pool)
            .await
            .map_err(from_sqlx)
            .map_err(|e| ValkyrinError::Introspection(format!("Failed to fetch PostgreSQL primary keys: {}", e)))?;
        let mut primary_key_orders: std::collections::HashMap<String, Vec<(String, usize)>> =
            std::collections::HashMap::new();
        for row in pk_rows {
            let table_name: String = row.get("table_name");
            let column_name: String = row.get("column_name");
            let ordinal_position: i32 = row.get("ordinal_position");
            primary_key_orders
                .entry(table_name)
                .or_default()
                .push((column_name, (ordinal_position - 1) as usize));
        }

        // Fetch index information for PostgreSQL
        let index_query = r#"
            SELECT
                i.relname AS index_name,
                t.relname AS table_name,
                a.attname AS column_name
            FROM pg_class t
            JOIN pg_index ix ON t.oid = ix.indrelid
            JOIN pg_class i ON i.oid = ix.indexrelid
            JOIN pg_attribute a ON a.attrelid = t.oid AND a.attnum = ANY(ix.indkey)
            JOIN pg_namespace n ON t.relnamespace = n.oid
            WHERE t.relkind = 'r'
              AND n.nspname = 'public'
              AND NOT ix.indisprimary
              AND NOT ix.indisunique
        "#;
        let index_rows = sqlx::query(index_query)
            .fetch_all(&self.pool)
            .await
            .map_err(from_sqlx)
            .map_err(|e| ValkyrinError::Introspection(format!("Failed to fetch PostgreSQL indexes: {}", e)))?;
        
        let mut indexed_columns: std::collections::HashMap<String, std::collections::HashSet<String>> = 
            std::collections::HashMap::new();
        for row in index_rows {
            let table_name: String = row.get("table_name");
            let column_name: String = row.get("column_name");
            indexed_columns
                .entry(table_name)
                .or_default()
                .insert(column_name);
        }

        for row in rows {
            let table_name: String = row.get("table_name");
            let column_name: String = row.get("column_name");
            let db_type: String = row.get("data_type");
            let is_nullable_str: String = row.get("is_nullable");
            let column_default: Option<String> = row.get("column_default");
            let udt_name: Option<String> = row.get("udt_name");

            if table_name != current_table_name && !current_table_name.is_empty() {
                entities.push(Entity {
                    id: uuid::Uuid::new_v4().to_string(),
                    name: current_table_name.clone(),
                    fields: current_fields.clone(),
                });
                current_fields.clear();
            }

            current_table_name = table_name.clone();

            let mapped_type = if db_type == "USER-DEFINED" {
                // PostgreSQL enum – fetch enum labels using the udt_name
                let enum_vals: Vec<String> = if let Some(ref type_name) = udt_name {
                    let enum_query = r#"SELECT enumlabel FROM pg_enum WHERE enumtypid = (SELECT oid FROM pg_type WHERE typname = $1)"#;
                    
                    sqlx::query(enum_query)
                        .bind(type_name)
                        .fetch_all(&self.pool)
                        .await
                        .map_err(from_sqlx)
                        .map_err(|e| ValkyrinError::Introspection(format!("Failed to fetch PostgreSQL enum values: {}", e)))?
                        .iter()
                        .map(|r| r.get::<String, _>("enumlabel"))
                        .collect()
                } else {
                    vec![]
                };
                DataType::Enum {
                    values: enum_vals,
                    type_name: udt_name.clone(),
                }
            } else {
                match db_type.as_str() {
                    "character varying" | "text" => DataType::String { max_length: None },
                    "integer" => DataType::Integer(crate::ir::IntSize::Standard),
                    "bigint" => DataType::Integer(crate::ir::IntSize::Big),
                    "smallint" => DataType::Integer(crate::ir::IntSize::Small),
                    "boolean" => DataType::Boolean,
                    "timestamp without time zone" | "timestamp with time zone" => DataType::DateTime,
                    "jsonb" | "json" => DataType::Json,
                    "numeric" | "decimal" => DataType::Decimal { precision: 10, scale: 2 },
                    "real" | "double precision" => DataType::Float,
                    "uuid" => DataType::Uuid,
                    "bytea" => DataType::Text,
                    _ => DataType::Text,
                }
            };

            let pk_order = primary_key_orders
                .get(&table_name)
                .and_then(|vec| vec.iter().find(|(col, _)| col == &column_name).map(|(_, ord)| *ord));

            let is_indexed = indexed_columns
                .get(&table_name)
                .map(|cols| cols.contains(&column_name))
                .unwrap_or(false);

            current_fields.push(Field {
                id: uuid::Uuid::new_v4().to_string(),
                name: column_name,
                data_type: mapped_type,
                constraints: crate::ir::Constraints {
                    is_primary_key: pk_order.is_some(),
                    primary_key_order: pk_order,
                    is_unique: false,
                    is_nullable: is_nullable_str == "YES",
                    is_indexed,
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

    async fn fetch_relations(&self) -> ValkyrinResult<Vec<DetectedRelation>> {
        let query = r#"
            SELECT
                tc.table_name AS source_table,
                kcu.column_name AS source_column,
                ccu.table_name AS target_table,
                ccu.column_name AS target_column
            FROM information_schema.table_constraints tc
            JOIN information_schema.key_column_usage kcu
              ON tc.constraint_name = kcu.constraint_name
              AND tc.table_schema = kcu.table_schema
            JOIN information_schema.constraint_column_usage ccu
              ON ccu.constraint_name = tc.constraint_name
              AND ccu.table_schema = tc.table_schema
            WHERE tc.constraint_type = 'FOREIGN KEY'
              AND tc.table_schema = 'public'
        "#;

        let rows = sqlx::query(query)
            .fetch_all(&self.pool)
            .await
            .map_err(from_sqlx)
            .map_err(|e| ValkyrinError::Introspection(format!("Failed to fetch PostgreSQL relations: {}", e)))?;
        let mut relations = Vec::new();
        for row in rows {
            relations.push(DetectedRelation {
                source_table: row.get("source_table"),
                source_column: row.get("source_column"),
                target_table: row.get("target_table"),
                target_column: row.get("target_column"),
            });
        }
        Ok(relations)
    }
}

// ──────────────────────────────────────────────
// MySQL
// ──────────────────────────────────────────────

pub struct MysqlIntrospector {
    pool: sqlx::Pool<sqlx::MySql>,
}

impl MysqlIntrospector {
    pub async fn new(url: &str) -> ValkyrinResult<Self> {
        let pool = sqlx::Pool::<sqlx::MySql>::connect(url)
            .await
            .map_err(from_sqlx)
            .map_err(|e| ValkyrinError::Database(format!("Failed to connect to MySQL: {}", e)))?;
        Ok(Self { pool })
    }
}

#[async_trait]
impl DatabaseIntrospector for MysqlIntrospector {
    async fn fetch_schema(&self) -> ValkyrinResult<Vec<Entity>> {
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

        let rows = sqlx::query(query)
            .fetch_all(&self.pool)
            .await
            .map_err(from_sqlx)
            .map_err(|e| ValkyrinError::Introspection(format!("Failed to fetch MySQL schema: {}", e)))?;
        let mut entities: Vec<Entity> = Vec::new();
        let mut current_table_name = String::new();
        let mut current_fields: Vec<Field> = Vec::new();

        let pk_query = r#"
            SELECT TABLE_NAME, COLUMN_NAME, ORDINAL_POSITION
            FROM INFORMATION_SCHEMA.KEY_COLUMN_USAGE
            WHERE TABLE_SCHEMA = DATABASE() AND CONSTRAINT_NAME = 'PRIMARY'
            ORDER BY TABLE_NAME, ORDINAL_POSITION
        "#;
        let pk_rows = sqlx::query(pk_query)
            .fetch_all(&self.pool)
            .await
            .map_err(from_sqlx)
            .map_err(|e| ValkyrinError::Introspection(format!("Failed to fetch MySQL primary keys: {}", e)))?;
        let mut primary_key_orders: std::collections::HashMap<String, Vec<(String, usize)>> =
            std::collections::HashMap::new();
        for row in pk_rows {
            let table_name: String = row.get("TABLE_NAME");
            let column_name: String = row.get("COLUMN_NAME");
            let ordinal_position: i32 = row.get("ORDINAL_POSITION");
            primary_key_orders
                .entry(table_name)
                .or_default()
                .push((column_name, (ordinal_position - 1) as usize));
        }

        // Fetch index information for MySQL
        let index_query = r#"
            SELECT TABLE_NAME, COLUMN_NAME
            FROM INFORMATION_SCHEMA.STATISTICS
            WHERE TABLE_SCHEMA = DATABASE()
              AND NON_UNIQUE = 1
              AND INDEX_NAME != 'PRIMARY'
        "#;
        let index_rows = sqlx::query(index_query)
            .fetch_all(&self.pool)
            .await
            .map_err(from_sqlx)
            .map_err(|e| ValkyrinError::Introspection(format!("Failed to fetch MySQL indexes: {}", e)))?;
        
        let mut indexed_columns: std::collections::HashMap<String, std::collections::HashSet<String>> = 
            std::collections::HashMap::new();
        for row in index_rows {
            let table_name: String = row.get("TABLE_NAME");
            let column_name: String = row.get("COLUMN_NAME");
            indexed_columns
                .entry(table_name)
                .or_default()
                .insert(column_name);
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

            let mapped_type = if column_type.starts_with("enum") {
                // MySQL ENUM – extract enum values from column_type like "enum('a','b')"
                let vals_str = column_type
                    .trim_start_matches("enum(")
                    .trim_end_matches(')');
                let enum_vals: Vec<String> = vals_str
                    .split(',')
                    .map(|s| s.trim().trim_matches('\'').to_string())
                    .collect();
                DataType::Enum {
                    values: enum_vals,
                    type_name: None,
                }
            } else {
                match column_type.as_str() {
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
                }
            };

            let pk_order = primary_key_orders
                .get(&table_name)
                .and_then(|vec| vec.iter().find(|(col, _)| col == &column_name).map(|(_, ord)| *ord));

            let is_indexed = indexed_columns
                .get(&table_name)
                .map(|cols| cols.contains(&column_name))
                .unwrap_or(false);

            current_fields.push(Field {
                id: uuid::Uuid::new_v4().to_string(),
                name: column_name,
                data_type: mapped_type,
                constraints: crate::ir::Constraints {
                    is_primary_key: pk_order.is_some(),
                    primary_key_order: pk_order,
                    is_unique: false,
                    is_nullable: is_nullable_str == "YES",
                    is_indexed,
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

    async fn fetch_relations(&self) -> ValkyrinResult<Vec<DetectedRelation>> {
        let query = r#"
            SELECT
                kcu.TABLE_NAME AS source_table,
                kcu.COLUMN_NAME AS source_column,
                kcu.REFERENCED_TABLE_NAME AS target_table,
                kcu.REFERENCED_COLUMN_NAME AS target_column
            FROM INFORMATION_SCHEMA.KEY_COLUMN_USAGE kcu
            WHERE kcu.TABLE_SCHEMA = DATABASE()
              AND kcu.REFERENCED_TABLE_NAME IS NOT NULL
        "#;

        let rows = sqlx::query(query)
            .fetch_all(&self.pool)
            .await
            .map_err(from_sqlx)
            .map_err(|e| ValkyrinError::Introspection(format!("Failed to fetch MySQL relations: {}", e)))?;
        let mut relations = Vec::new();
        for row in rows {
            relations.push(DetectedRelation {
                source_table: row.get("source_table"),
                source_column: row.get("source_column"),
                target_table: row.get("target_table"),
                target_column: row.get("target_column"),
            });
        }
        Ok(relations)
    }
}

// ──────────────────────────────────────────────
// SQLite
// ──────────────────────────────────────────────

pub struct SqliteIntrospector {
    pool: sqlx::Pool<sqlx::Sqlite>,
}

impl SqliteIntrospector {
    pub async fn new(url: &str) -> ValkyrinResult<Self> {
        if !sqlx::Sqlite::database_exists(url).await.unwrap_or(false) {
            sqlx::Sqlite::create_database(url).await
                .map_err(from_sqlx)
                .map_err(|e| ValkyrinError::Database(format!("Failed to create SQLite database: {}", e)))?;
        }
        let pool = sqlx::Pool::<sqlx::Sqlite>::connect(url)
            .await
            .map_err(from_sqlx)
            .map_err(|e| ValkyrinError::Database(format!("Failed to connect to SQLite: {}", e)))?;
        Ok(Self { pool })
    }
}

#[async_trait]
impl DatabaseIntrospector for SqliteIntrospector {
    async fn fetch_schema(&self) -> ValkyrinResult<Vec<Entity>> {
        let tables_query =
            r#"SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%';"#;
        let table_rows = sqlx::query(tables_query)
            .fetch_all(&self.pool)
            .await
            .map_err(from_sqlx)
            .map_err(|e| ValkyrinError::Introspection(format!("Failed to fetch SQLite tables: {}", e)))?;

        let mut entities: Vec<Entity> = Vec::new();

        // Validate table name against identifier regex to prevent SQL injection
        let identifier_regex = regex::Regex::new(r"^[a-zA-Z_][a-zA-Z0-9_]*$")
            .map_err(|e| ValkyrinError::Internal(format!("Regex compilation failed: {}", e)))?;

        for table_row in table_rows {
            let table_name: String = table_row.get("name");
            
            if !identifier_regex.is_match(&table_name) {
                return Err(ValkyrinError::Schema(
                    format!("Invalid table name '{}': does not match identifier pattern", table_name)
                ));
            }
            
            let columns_query = format!("PRAGMA table_info({});", table_name);
            let column_rows = sqlx::query(&columns_query)
                .fetch_all(&self.pool)
                .await
                .map_err(from_sqlx)
                .map_err(|e| ValkyrinError::Introspection(format!("Failed to fetch SQLite columns for {}: {}", table_name, e)))?;

            // Fetch index information for SQLite
            let index_list_query = format!("PRAGMA index_list({});", table_name);
            let index_list_rows = sqlx::query(&index_list_query)
                .fetch_all(&self.pool)
                .await
                .map_err(from_sqlx)
                .map_err(|e| ValkyrinError::Introspection(format!("Failed to fetch SQLite indexes for {}: {}", table_name, e)))?;
            
            let mut indexed_columns: std::collections::HashSet<String> = std::collections::HashSet::new();
            for idx_row in index_list_rows {
                let idx_name: String = idx_row.get("name");
                // Skip primary key indexes (they're handled separately)
                let idx_info_query = format!("PRAGMA index_info({});", idx_name);
                let idx_info_rows = sqlx::query(&idx_info_query)
                    .fetch_all(&self.pool)
                    .await
                    .map_err(from_sqlx)
                    .map_err(|e| ValkyrinError::Introspection(format!("Failed to fetch SQLite index info for {}: {}", idx_name, e)))?;
                
                for info_row in idx_info_rows {
                    let col_name: String = info_row.get("name");
                    indexed_columns.insert(col_name);
                }
            }

            let mut fields: Vec<Field> = Vec::new();

            for col_row in column_rows {
                let column_name: String = col_row.get("name");
                let sql_type: String = col_row.get("type");
                let is_nullable: i32 = col_row.get("notnull");
                let pk_info: i32 = col_row.get("pk");
                let default_value: Option<String> = col_row.get("dflt_value");

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

                let pk_order = if pk_info > 0 { Some((pk_info - 1) as usize) } else { None };

                let is_indexed = indexed_columns.contains(&column_name);

                fields.push(Field {
                    id: uuid::Uuid::new_v4().to_string(),
                    name: column_name,
                    data_type: mapped_type,
                    constraints: crate::ir::Constraints {
                        is_primary_key: pk_order.is_some(),
                        primary_key_order: pk_order,
                        is_unique: false,
                        is_nullable: is_nullable == 0,
                        is_indexed,
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

    async fn fetch_relations(&self) -> ValkyrinResult<Vec<DetectedRelation>> {
        let tables_query =
            r#"SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%';"#;
        let table_rows = sqlx::query(tables_query)
            .fetch_all(&self.pool)
            .await
            .map_err(from_sqlx)
            .map_err(|e| ValkyrinError::Introspection(format!("Failed to fetch SQLite tables for relations: {}", e)))?;

        let mut relations = Vec::new();
        for table_row in table_rows {
            let table_name: String = table_row.get("name");
            let fk_query = format!("PRAGMA foreign_key_list({});", table_name);
            let fk_rows = sqlx::query(&fk_query)
                .fetch_all(&self.pool)
                .await
                .map_err(from_sqlx)
                .map_err(|e| ValkyrinError::Introspection(format!("Failed to fetch SQLite FKs for {}: {}", table_name, e)))?;

            for row in fk_rows {
                let from_col: String = row.get("from");
                let to_table: String = row.get("table");
                let to_col: String = row.get("to");
                relations.push(DetectedRelation {
                    source_table: table_name.clone(),
                    source_column: from_col,
                    target_table: to_table,
                    target_column: to_col,
                });
            }
        }
        Ok(relations)
    }
}

// ──────────────────────────────────────────────
// Migration
// ──────────────────────────────────────────────

/// Represents a database migration with up and down SQL.
#[derive(Debug, Clone)]
pub struct Migration {
    pub version: String,
    pub name: String,
    pub up_sql: String,
    pub down_sql: String,
    pub created_at: u64,
}

// ──────────────────────────────────────────────
// Diff Engine
// ──────────────────────────────────────────────

pub struct SyncEngine;

impl SyncEngine {
    /// Calculates a detailed, column-level diff between a live database schema and a local canvas.
    /// The diff is bidirectional — it detects changes in both directions.
    pub fn calculate_detailed_diff(
        live_schema: &[Entity],
        local_schema: &[Entity],
    ) -> DetailedDiff {
        let mut new_tables = Vec::new();
        let mut removed_tables = Vec::new();
        let mut modified_tables = Vec::new();

        // Find new and modified tables
        for live_table in live_schema {
            match local_schema
                .iter()
                .find(|loc| loc.name == live_table.name)
            {
                None => {
                    // Table exists in DB but not on canvas
                    new_tables.push(live_table.clone());
                }
                Some(local_table) => {
                    // Both sides have this table — diff the columns
                    let td = Self::diff_table_columns(local_table, live_table);
                    if td.has_changes() {
                        modified_tables.push(td);
                    }
                }
            }
        }

        // Find removed tables (exist on canvas but not in DB)
        for local_table in local_schema {
            if !live_schema
                .iter()
                .any(|live| live.name == local_table.name)
            {
                removed_tables.push(local_table.name.clone());
            }
        }

        let detected_relations = Vec::new(); // filled later by fetch_relations

        DetailedDiff {
            new_tables,
            removed_tables,
            modified_tables,
            detected_relations,
        }
    }

    /// Compares columns between a local (canvas) table and a live database table.
    fn diff_table_columns(local: &Entity, live: &Entity) -> TableDiff {
        let mut adds: Vec<Field> = Vec::new();
        let mut removes: Vec<Field> = Vec::new();
        let mut changes: Vec<ColumnChange> = Vec::new();

        // Find columns added in DB or modified
        for live_field in &live.fields {
            match local.fields.iter().find(|f| f.name == live_field.name) {
                None => {
                    adds.push(live_field.clone());
                }
                Some(local_field) => {
                    // Compare types - special handling for PostgreSQL enums
                    if local_field.data_type != live_field.data_type {
                        // Check if both are enums with same type_name (PostgreSQL native enum)
                        let is_enum_type_change = match (&local_field.data_type, &live_field.data_type) {
                            (DataType::Enum { type_name: Some(local_type), values: local_vals },
                             DataType::Enum { type_name: Some(live_type), values: live_vals }) => {
                                local_type == live_type && local_vals != live_vals
                            }
                            _ => false,
                        };
                        
                        if is_enum_type_change {
                            // Extract added enum values
                            if let (DataType::Enum { values: local_vals, .. }, DataType::Enum { values: live_vals, type_name: Some(live_type) }) = 
                                (&local_field.data_type, &live_field.data_type) {
                                let added_values: Vec<String> = live_vals.iter()
                                    .filter(|v| !local_vals.contains(*v))
                                    .cloned()
                                    .collect();
                                if !added_values.is_empty() {
                                    changes.push(ColumnChange::EnumTypeChanged {
                                        name: live_field.name.clone(),
                                        type_name: live_type.clone(),
                                        added_values,
                                    });
                                }
                            }
                        } else {
                            changes.push(ColumnChange::TypeChanged {
                                name: live_field.name.clone(),
                                from: local_field.data_type.clone(),
                                to: live_field.data_type.clone(),
                            });
                        }
                    }
                    // Compare nullable
                    if local_field.constraints.is_nullable != live_field.constraints.is_nullable {
                        changes.push(ColumnChange::NullableChanged {
                            name: live_field.name.clone(),
                            was_nullable: local_field.constraints.is_nullable,
                            is_nullable: live_field.constraints.is_nullable,
                        });
                    }
                    // Compare unique
                    if local_field.constraints.is_unique != live_field.constraints.is_unique {
                        changes.push(ColumnChange::UniqueChanged {
                            name: live_field.name.clone(),
                            was_unique: local_field.constraints.is_unique,
                            is_unique: live_field.constraints.is_unique,
                        });
                    }
                    // Compare default
                    if local_field.constraints.default_value
                        != live_field.constraints.default_value
                    {
                        changes.push(ColumnChange::DefaultChanged {
                            name: live_field.name.clone(),
                            before: local_field.constraints.default_value.clone(),
                            after: live_field.constraints.default_value.clone(),
                        });
                    }
                    // Compare primary key order
                    if local_field.constraints.primary_key_order != live_field.constraints.primary_key_order {
                        changes.push(ColumnChange::TypeChanged {
                            name: live_field.name.clone(),
                            from: DataType::String { max_length: None }, // placeholder for PK order change
                            to: DataType::String { max_length: None },
                        });
                    }
                }
            }
        }

        // Find columns removed from DB (still in local but not in live)
        for local_field in &local.fields {
            if !live.fields.iter().any(|f| f.name == local_field.name) {
                removes.push(local_field.clone());
            }
        }

        TableDiff {
            table_name: live.name.clone(),
            adds,
            removes,
            changes,
        }
    }

/// Generates a human-readable diff report string
    pub fn format_diff_report(diff: &DetailedDiff) -> String {
        let mut report = String::new();
        let has_content = !diff.new_tables.is_empty()
            || !diff.removed_tables.is_empty()
            || !diff.modified_tables.is_empty();

        if !has_content {
            report.push_str("   ✅ Canvas is perfectly synced with the live database.\n");
            return report;
        }

        // New tables
        if !diff.new_tables.is_empty() {
            report.push_str(&format!(
                "   ✨ {} new table(s) discovered in database:\n",
                diff.new_tables.len()
            ));
            for t in &diff.new_tables {
                report.push_str(&format!(
                    "      + {} ({} columns)\n",
                    t.name,
                    t.fields.len()
                ));
            }
        }

        // Removed tables
        if !diff.removed_tables.is_empty() {
            report.push_str(&format!(
                "   🗑️  {} table(s) removed from database:\n",
                diff.removed_tables.len()
            ));
            for name in &diff.removed_tables {
                report.push_str(&format!("      - {}\n", name));
            }
            report
                .push_str("      ⚠️  Use --confirm to remove them from the canvas.\n");
        }

        // Modified tables
        if !diff.modified_tables.is_empty() {
            report.push_str(&format!(
                "   🔄 {} table(s) with column changes:\n",
                diff.modified_tables.len()
            ));
            for td in &diff.modified_tables {
                report.push_str(&format!("      ~ {}\n", td.table_name));
                for f in &td.adds {
                    report.push_str(&format!(
                        "         + {}  ({})\n",
                        f.name, &Self::type_string(&f.data_type)
                    ));
                }
                for f in &td.removes {
                    report.push_str(&format!("         - {}  (removed from DB)\n", f.name));
                }
                for c in &td.changes {
                    match c {
                        ColumnChange::TypeChanged { name, from, to } => {
                            report.push_str(&format!(
                                "         ~ {} type: {} → {}\n",
                                name,
                                Self::type_string(from),
                                Self::type_string(to)
                            ));
                        }
                        ColumnChange::NullableChanged {
                            name,
                            was_nullable,
                            is_nullable,
                        } => {
                            let arrow = if *was_nullable { "nullable" } else { "not-null" };
                            let arrow2 = if *is_nullable { "nullable" } else { "not-null" };
                            report.push_str(&format!(
                                "         ~ {} nullable: {} → {}\n",
                                name, arrow, arrow2
                            ));
                        }
                        ColumnChange::UniqueChanged {
                            name,
                            was_unique,
                            is_unique,
                        } => {
                            let was = if *was_unique { "unique" } else { "not-unique" };
                            let now = if *is_unique { "unique" } else { "not-unique" };
                            report.push_str(&format!(
                                "         ~ {} unique: {} → {}\n",
                                name, was, now
                            ));
                        }
                        ColumnChange::DefaultChanged { name, before, after } => {
                            let b = before.clone().unwrap_or_else(|| "none".to_string());
                            let a = after.clone().unwrap_or_else(|| "none".to_string());
                            report.push_str(&format!(
                                "         ~ {} default: {} → {}\n",
                                name, b, a
                            ));
                        }
                        ColumnChange::EnumTypeChanged { name, type_name, added_values } => {
                            report.push_str(&format!(
                                "         ~ {} enum type '{}' added values: {}\n",
                                name, type_name, added_values.join(", ")
                            ));
                        }
                    }
                }
            }
        }

        report
    }

    /// Formats a DataType to a short readable string
    fn type_string(dt: &DataType) -> String {
        match dt {
            DataType::String { .. } => "string".to_string(),
            DataType::Text => "text".to_string(),
            DataType::Integer(s) => match s {
                crate::ir::IntSize::Small => "smallint".to_string(),
                crate::ir::IntSize::Standard => "int".to_string(),
                crate::ir::IntSize::Big => "bigint".to_string(),
            },
            DataType::Float => "float".to_string(),
            DataType::Decimal { precision, scale } => {
                format!("decimal({},{})", precision, scale)
            }
            DataType::Boolean => "boolean".to_string(),
            DataType::DateTime => "datetime".to_string(),
            DataType::Json => "json".to_string(),
            DataType::Uuid => "uuid".to_string(),
            DataType::Enum { values, type_name: _ } => format!("enum({})", values.join("|")),
        }
    }

    /// Generates SQL migration statements to make the database match the canvas.
    pub fn generate_migration(
        local_schema: &[Entity],
        diff: &DetailedDiff,
        db_type: DatabaseType,
    ) -> Vec<String> {
        let mut statements = Vec::new();

        // DROP tables that were removed from canvas
        for table_name in &diff.removed_tables {
            statements.push(format!("DROP TABLE IF EXISTS \"{}\";", table_name));
        }

        // Collect PostgreSQL enum CREATE TYPE statements (must run before table creation)
        if db_type == DatabaseType::PostgreSQL {
            for entity in diff.new_tables.iter() {
                for field in &entity.fields {
                    if let DataType::Enum { values, type_name: Some(enum_type_name) } = &field.data_type {
                        if !values.is_empty() {
                            let enum_vals = values.iter().map(|v| format!("'{}'", v)).collect::<Vec<_>>().join(", ");
                            statements.push(format!("CREATE TYPE {} AS ENUM ({});", enum_type_name, enum_vals));
                        }
                    }
                }
            }
            for td in &diff.modified_tables {
                for change in &td.changes {
                    if let ColumnChange::EnumTypeChanged { type_name, added_values, .. } = change {
                        if !added_values.is_empty() {
                            for val in added_values {
                                statements.push(format!("ALTER TYPE {} ADD VALUE '{}';", type_name, val));
                            }
                        }
                    }
                }
            }
        }

        // CREATE new tables
        for entity in diff.new_tables.iter().chain(
            local_schema
                .iter()
                .filter(|e| !diff.removed_tables.contains(&e.name)),
        ) {
            let cols: Vec<String> = entity
                .fields
                .iter()
                .map(|f| {
                    let sql_type = Self::ir_type_to_sql(&f.data_type, db_type);
                    let mut parts = vec![format!("    \"{}\" {}", f.name, sql_type)];
                    if f.constraints.is_primary_key {
                        parts.push("PRIMARY KEY".to_string());
                    }
                    if !f.constraints.is_nullable {
                        parts.push("NOT NULL".to_string());
                    }
                    if f.constraints.is_unique {
                        parts.push("UNIQUE".to_string());
                    }
                    if let Some(ref def) = f.constraints.default_value {
                        parts.push(format!("DEFAULT {}", def));
                    }
                    parts.join(" ")
                })
                .collect();

            // Only generate CREATE for tables not yet in DB
            if !diff.new_tables.iter().any(|e| e.name == entity.name) {
                // Only ALTER existing tables
                continue;
            }

            statements.push(format!(
                "CREATE TABLE \"{}\" (\n{},\n    \"id\" UUID PRIMARY KEY\n);",
                entity.name,
                cols.join(",\n"),
            ));
        }

        // ALTER TABLE for column changes
        for td in &diff.modified_tables {
            for f in &td.adds {
                let sql_type = Self::ir_type_to_sql(&f.data_type, db_type);
                let nullable = if f.constraints.is_nullable {
                    ""
                } else {
                    " NOT NULL"
                };
                statements.push(format!(
                    "ALTER TABLE \"{}\" ADD COLUMN \"{}\" {}{};",
                    td.table_name, f.name, sql_type, nullable
                ));
            }
            for f in &td.removes {
                statements.push(format!(
                    "ALTER TABLE \"{}\" DROP COLUMN \"{}\";",
                    td.table_name, f.name
                ));
            }
        }

        statements
    }

    /// Generates reverse migration statements (DOWN) to rollback the up migration.
    pub fn generate_down_migration(
        local_schema: &[Entity],
        diff: &DetailedDiff,
        db_type: DatabaseType,
    ) -> Vec<String> {
        let mut statements = Vec::new();

        // For removed tables in diff (tables that existed in DB but not in canvas),
        // the DOWN migration should CREATE them back
        for table_name in &diff.removed_tables {
            // Find the table in local_schema (the canvas has the original definition)
            if let Some(entity) = local_schema.iter().find(|e| e.name == *table_name) {
                let cols: Vec<String> = entity
                    .fields
                    .iter()
                    .map(|f| {
                        let sql_type = Self::ir_type_to_sql(&f.data_type, db_type);
                        let mut parts = vec![format!("    \"{}\" {}", f.name, sql_type)];
                        if f.constraints.is_primary_key {
                            parts.push("PRIMARY KEY".to_string());
                        }
                        if !f.constraints.is_nullable {
                            parts.push("NOT NULL".to_string());
                        }
                        if f.constraints.is_unique {
                            parts.push("UNIQUE".to_string());
                        }
                        if let Some(ref def) = f.constraints.default_value {
                            parts.push(format!("DEFAULT {}", def));
                        }
                        parts.join(" ")
                    })
                    .collect();

                statements.push(format!(
                    "CREATE TABLE \"{}\" (\n{});",
                    table_name,
                    cols.join(",\n"),
                ));
            }
        }

        // For new tables in diff (tables that exist in DB but not in canvas),
        // the DOWN migration should DROP them
        for entity in &diff.new_tables {
            statements.push(format!("DROP TABLE IF EXISTS \"{}\";", entity.name));
        }

        // For modified tables, reverse the column changes
        for td in &diff.modified_tables {
            // Reverse: DROP columns that were ADDED
            for f in &td.adds {
                statements.push(format!(
                    "ALTER TABLE \"{}\" DROP COLUMN IF EXISTS \"{}\";",
                    td.table_name, f.name
                ));
            }
            // Reverse: ADD back columns that were REMOVED
            for f in &td.removes {
                let sql_type = Self::ir_type_to_sql(&f.data_type, db_type);
                let nullable = if f.constraints.is_nullable {
                    ""
                } else {
                    " NOT NULL"
                };
                statements.push(format!(
                    "ALTER TABLE \"{}\" ADD COLUMN \"{}\" {}{};",
                    td.table_name, f.name, sql_type, nullable
                ));
            }
            // Note: Type changes, nullable changes, etc. are harder to reverse perfectly
            // without storing the original schema. For now, we skip reversing modifications.
        }

        statements
    }

    /// Creates a complete Migration struct with both up and down SQL.
    pub fn create_migration(
        local_schema: &[Entity],
        diff: &DetailedDiff,
        db_type: DatabaseType,
        name: &str,
    ) -> Option<Migration> {
        let up_statements = Self::generate_migration(local_schema, diff, db_type);
        let down_statements = Self::generate_down_migration(local_schema, diff, db_type);

        if up_statements.is_empty() && down_statements.is_empty() {
            return None;
        }

        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        let version = format!("{}_{}", timestamp, name.replace(' ', "_"));

        Some(Migration {
            version: version.clone(),
            name: name.to_string(),
            up_sql: up_statements.join("\n\n"),
            down_sql: down_statements.join("\n\n"),
            created_at: timestamp,
        })
    }

    /// Writes migration statements to a timestamped SQL file in the migrations/ directory.
    pub fn write_migration_file(
        local_schema: &[Entity],
        diff: &DetailedDiff,
        statements: &[String],
        db_type: DatabaseType,
    ) -> ValkyrinResult<Option<String>> {
        if statements.is_empty() {
            return Ok(None);
        }

        let migration_dir = "migrations";
        fs::create_dir_all(migration_dir)
            .map_err(|e| ValkyrinError::Io(format!("Failed to create migrations directory: {}", e)))?;

        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        let db_suffix = match db_type {
            DatabaseType::PostgreSQL => "postgres",
            DatabaseType::MySQL => "mysql",
            DatabaseType::SQLite => "sqlite",
        };

        let filename = format!("{}/{}_migration_{}.sql", migration_dir, db_suffix, timestamp);
        let mut content = String::new();
        content.push_str("-- Valkyrin Auto-generated Migration\n");
        content.push_str(&format!("-- Database: {}\n", db_suffix));
        content.push_str(&format!("-- Timestamp: {}\n\n", timestamp));
        content.push_str("-- UP\n");

        for stmt in statements {
            content.push_str(stmt);
            content.push_str("\n\n");
        }

        fs::write(&filename, content)
            .map_err(|e| ValkyrinError::Io(format!("Failed to write migration file: {}", e)))?;
        println!("📝 Migration written to: {}", filename);

        // Also write DOWN migration file
        let down_filename = format!("{}/{}_migration_{}.down.sql", migration_dir, db_suffix, timestamp);
        let down_sql = Self::generate_down_migration(local_schema, diff, db_type);
        if !down_sql.is_empty() {
            let mut down_content = String::new();
            down_content.push_str("-- Valkyrin Auto-generated Migration (DOWN)\n");
            down_content.push_str(&format!("-- Database: {}\n", db_suffix));
            down_content.push_str(&format!("-- Timestamp: {}\n\n", timestamp));
            down_content.push_str("-- DOWN\n");

            for stmt in down_sql {
                down_content.push_str(&stmt);
                down_content.push_str("\n\n");
            }

            fs::write(&down_filename, down_content)
                .map_err(|e| ValkyrinError::Io(format!("Failed to write down migration file: {}", e)))?;
            println!("📝 Down migration written to: {}", down_filename);
        }

        Ok(Some(filename))
    }

    /// Executes a list of migration statements against the target database.
    pub async fn execute_migration(
        db_url: &str,
        db_type: DatabaseType,
        statements: &[String],
    ) -> ValkyrinResult<()> {
        if statements.is_empty() {
            println!("✅ No migration statements to execute.");
            return Ok(());
        }

        println!("🚀 Executing migration against {}...", match db_type {
            DatabaseType::PostgreSQL => "PostgreSQL",
            DatabaseType::MySQL => "MySQL",
            DatabaseType::SQLite => "SQLite",
        });

        match db_type {
            DatabaseType::PostgreSQL => {
                let pool = sqlx::Pool::<sqlx::Postgres>::connect(db_url)
                    .await
                    .map_err(from_sqlx)
                    .map_err(|e| ValkyrinError::Database(format!("Failed to connect to PostgreSQL: {}", e)))?;
                for stmt in statements {
                    println!("   ▶ {}", stmt);
                    sqlx::query(stmt).execute(&pool).await
                        .map_err(from_sqlx)
                        .map_err(|e| ValkyrinError::Migration(format!("Failed to execute: {}: {}", stmt, e)))?;
                }
            }
            DatabaseType::MySQL => {
                let pool = sqlx::Pool::<sqlx::MySql>::connect(db_url)
                    .await
                    .map_err(from_sqlx)
                    .map_err(|e| ValkyrinError::Database(format!("Failed to connect to MySQL: {}", e)))?;
                for stmt in statements {
                    println!("   ▶ {}", stmt);
                    sqlx::query(stmt).execute(&pool).await
                        .map_err(from_sqlx)
                        .map_err(|e| ValkyrinError::Migration(format!("Failed to execute: {}: {}", stmt, e)))?;
                }
            }
            DatabaseType::SQLite => {
                let pool = sqlx::Pool::<sqlx::Sqlite>::connect(db_url)
                    .await
                    .map_err(from_sqlx)
                    .map_err(|e| ValkyrinError::Database(format!("Failed to connect to SQLite: {}", e)))?;
                for stmt in statements {
                    println!("   ▶ {}", stmt);
                    sqlx::query(stmt).execute(&pool).await
                        .map_err(from_sqlx)
                        .map_err(|e| ValkyrinError::Migration(format!("Failed to execute: {}: {}", stmt, e)))?;
                }
            }
        }

        println!("✅ Migration executed successfully.");
        Ok(())
    }

    /// Converts a Valkyrin DataType to an SQL type string for the given database.
    fn ir_type_to_sql(dt: &DataType, db_type: DatabaseType) -> String {
        match dt {
            DataType::String { max_length } => match max_length {
                Some(n) => format!("VARCHAR({})", n),
                None => "VARCHAR(255)".to_string(),
            },
            DataType::Text => "TEXT".to_string(),
            DataType::Integer(crate::ir::IntSize::Small) => match db_type {
                DatabaseType::PostgreSQL => "SMALLINT".to_string(),
                DatabaseType::MySQL => "SMALLINT".to_string(),
                DatabaseType::SQLite => "INTEGER".to_string(),
            },
            DataType::Integer(crate::ir::IntSize::Standard) => match db_type {
                DatabaseType::PostgreSQL => "INTEGER".to_string(),
                DatabaseType::MySQL => "INT".to_string(),
                DatabaseType::SQLite => "INTEGER".to_string(),
            },
            DataType::Integer(crate::ir::IntSize::Big) => match db_type {
                DatabaseType::PostgreSQL => "BIGINT".to_string(),
                DatabaseType::MySQL => "BIGINT".to_string(),
                DatabaseType::SQLite => "INTEGER".to_string(),
            },
            DataType::Float => "FLOAT".to_string(),
            DataType::Decimal { precision, scale } => {
                format!("DECIMAL({},{})", precision, scale)
            }
            DataType::Boolean => match db_type {
                DatabaseType::PostgreSQL => "BOOLEAN".to_string(),
                DatabaseType::MySQL => "TINYINT(1)".to_string(),
                DatabaseType::SQLite => "INTEGER".to_string(),
            },
            DataType::DateTime => match db_type {
                DatabaseType::PostgreSQL => "TIMESTAMP".to_string(),
                DatabaseType::MySQL => "DATETIME".to_string(),
                DatabaseType::SQLite => "TEXT".to_string(),
            },
            DataType::Json => match db_type {
                DatabaseType::PostgreSQL => "JSONB".to_string(),
                DatabaseType::MySQL => "JSON".to_string(),
                DatabaseType::SQLite => "TEXT".to_string(),
            },
            DataType::Uuid => match db_type {
                DatabaseType::PostgreSQL => "UUID".to_string(),
                DatabaseType::MySQL => "CHAR(36)".to_string(),
                DatabaseType::SQLite => "TEXT".to_string(),
            },
            DataType::Enum { values, type_name } => match db_type {
                DatabaseType::PostgreSQL => {
                    // PostgreSQL: use native enum type name if available
                    type_name.clone().unwrap_or_else(|| "VARCHAR(255)".to_string())
                }
                DatabaseType::MySQL => {
                    format!("ENUM('{}')", values.join("','"))
                }
                DatabaseType::SQLite => "TEXT".to_string(),
            },
        }
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

    /// Runs a migration file (or the latest migration) against the target database.
    pub async fn run_migrations(
        db_url: &str,
        explicit_db_type: Option<&str>,
        migration_file: Option<&str>,
    ) -> ValkyrinResult<()> {
        // Determine DB type
        let db_type = if let Some(db_type_str) = explicit_db_type {
            match db_type_str.to_lowercase().as_str() {
                "postgres" | "postgresql" => DatabaseType::PostgreSQL,
                "mysql" => DatabaseType::MySQL,
                "sqlite" => DatabaseType::SQLite,
                _ => {
                    return Err(ValkyrinError::Config(
                        format!("Unknown database type: '{}'. Use 'postgres', 'mysql', or 'sqlite'.", db_type_str)
                    ))
                }
            }
        } else {
            DatabaseType::from_url(db_url)?
        };

        // Resolve migration file paths first (common for all DB types)
        let candidates = if let Some(p) = migration_file {
            vec![std::path::PathBuf::from(p)]
        } else {
            // Find all migration files for the DB type
            let suffix = match db_type {
                DatabaseType::PostgreSQL => "postgres",
                DatabaseType::MySQL => "mysql",
                DatabaseType::SQLite => "sqlite",
            };
            let migration_dir = std::path::Path::new("migrations");
            let mut candidates: Vec<std::path::PathBuf> = fs::read_dir(migration_dir)
                .map_err(|e| ValkyrinError::Io(format!("Failed to read migrations directory: {}", e)))?
                .filter_map(|e| e.ok())
                .map(|e| e.path())
                .filter(|p| {
                    p.extension()
                        .and_then(|ext| ext.to_str())
                        .map(|ext| ext.eq_ignore_ascii_case("sql"))
                        .unwrap_or(false)
                })
                .filter(|p| {
                    p.file_name()
                        .and_then(|n| n.to_str())
                        .map(|n| n.contains(suffix))
                        .unwrap_or(false)
                })
                .collect();
            // Sort by filename (timestamp part) ascending
            candidates.sort_by(|a, b| a.cmp(b));
            candidates
        };

        // Now handle each database type separately with their specific pool types
        match db_type {
            DatabaseType::PostgreSQL => {
                // Create connection pool and acquire lock
                let pool = sqlx::Pool::<sqlx::Postgres>::connect(db_url)
                    .await
                    .map_err(from_sqlx)
                    .map_err(|e| ValkyrinError::Database(format!("Failed to connect to PostgreSQL: {}", e)))?;
                let mut conn = pool.acquire()
                    .await
                    .map_err(from_sqlx)
                    .map_err(|e| ValkyrinError::Database(format!("Failed to acquire PostgreSQL connection: {}", e)))?;
                pg_advisory_lock(&mut conn, 0x76616C6B7972696E).await
                    .map_err(|e| ValkyrinError::Database(format!("Failed to acquire advisory lock: {}", e)))?;
                create_migration_table(&mut conn).await
                    .map_err(|e| ValkyrinError::Migration(format!("Failed to create migration table: {}", e)))?;

                // Get already applied migrations from the database (use pool as executor)
                let applied_migrations: Vec<MigrationRecord> = sqlx::query_as(
                    "SELECT version, name, checksum, applied_at, success FROM _valkyrin_migrations"
                )
                .fetch_all(&pool)
                .await
                .map_err(from_sqlx)
                .map_err(|e| ValkyrinError::Introspection(format!("Failed to fetch applied migrations: {}", e)))?;
                
                // Track which migrations have been applied with their checksums
                let mut applied: HashMap<String, String> = HashMap::new();
                for record in applied_migrations {
                    applied.insert(record.version.clone(), record.checksum);
                }
                
                // Execute pending migrations
                for path in candidates {
                    let file_name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
                    let version = file_name.to_string();
                    
                    // Read the migration file first to compute checksum
                    let sql = fs::read_to_string(&path)
                        .map_err(|e| ValkyrinError::Io(format!("Failed to read migration file {}: {}", path.display(), e)))?;
                    let checksum = {
                        let mut hasher = Sha256::new();
                        hasher.update(sql.as_bytes());
                        format!("{:x}", hasher.finalize())
                    };
                    
                    // Check if already applied
                    if let Some(stored_checksum) = applied.get(&version) {
                        // Validate checksum - reject if migration file was modified after apply
                        if stored_checksum != &checksum {
                            return Err(ValkyrinError::Migration(
                                format!("Migration {} has been modified after being applied (checksum mismatch: stored={}, computed={}). Refusing to re-apply.", file_name, stored_checksum, checksum)
                            ));
                        }
                        println!("⏭️  Migration {} already applied (checksum verified)", file_name);
                        continue;
                    }
                    
                    // Execute the migration in a transaction
                    let result = sqlx::query(&sql)
                        .execute(&pool)
                        .await;
                    
                    // Record the result
                    match result {
                        Ok(_) => {
                            println!("✅  Applied migration: {}", file_name);
                            sqlx::query(
                                "INSERT INTO _valkyrin_migrations (version, name, checksum, success) VALUES ($1, $2, $3, $4)"
                            )
                            .bind(&version)
                            .bind(file_name)
                            .bind(&checksum)
                            .bind(true)
                            .execute(&pool)
                            .await
                            .map_err(from_sqlx)
                            .map_err(|e| ValkyrinError::Migration(format!("Failed to record migration: {}", e)))?;
                        }
                        Err(e) => {
                            println!("❌  Failed to apply migration: {}", file_name);
                            sqlx::query(
                                "INSERT INTO _valkyrin_migrations (version, name, checksum, success) VALUES ($1, $2, $3, $4)"
                            )
                            .bind(&version)
                            .bind(file_name)
                            .bind(&checksum)
                            .bind(false)
                            .execute(&pool)
                            .await
                            .map_err(from_sqlx)
                            .map_err(|e| ValkyrinError::Migration(format!("Failed to record failed migration: {}", e)))?;
                            return Err(ValkyrinError::Migration(format!("Migration failed: {}", e)));
                        }
                    }
                }
        
        Ok(())
            }
            DatabaseType::MySQL => {
                // Create connection pool and acquire lock
                let pool = sqlx::Pool::<sqlx::MySql>::connect(db_url)
                    .await
                    .map_err(from_sqlx)
                    .map_err(|e| ValkyrinError::Database(format!("Failed to connect to MySQL: {}", e)))?;
                let mut conn = pool.acquire()
                    .await
                    .map_err(from_sqlx)
                    .map_err(|e| ValkyrinError::Database(format!("Failed to acquire MySQL connection: {}", e)))?;
                mysql_lock(&mut conn, "valkyrin_migration_lock").await
                    .map_err(|e| ValkyrinError::Database(format!("Failed to acquire MySQL lock: {}", e)))?;
                create_migration_table_mysql(&mut conn).await
                    .map_err(|e| ValkyrinError::Migration(format!("Failed to create migration table: {}", e)))?;

                // Get already applied migrations from the database (use pool as executor)
                let applied_migrations: Vec<MigrationRecord> = sqlx::query_as(
                    "SELECT version, name, checksum, applied_at, success FROM _valkyrin_migrations"
                )
                .fetch_all(&pool)
                .await
                .map_err(from_sqlx)
                .map_err(|e| ValkyrinError::Introspection(format!("Failed to fetch applied migrations: {}", e)))?;
                
                // Track which migrations have been applied with their checksums
                let mut applied: HashMap<String, String> = HashMap::new();
                for record in applied_migrations {
                    applied.insert(record.version.clone(), record.checksum);
                }
                
                // Execute pending migrations
                for path in candidates {
                    let file_name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
                    let version = file_name.to_string();
                    
                    // Read the migration file first to compute checksum
                    let sql = fs::read_to_string(&path)
                        .map_err(|e| ValkyrinError::Io(format!("Failed to read migration file {}: {}", path.display(), e)))?;
                    let checksum = {
                        let mut hasher = Sha256::new();
                        hasher.update(sql.as_bytes());
                        format!("{:x}", hasher.finalize())
                    };
                    
                    // Check if already applied
                    if let Some(stored_checksum) = applied.get(&version) {
                        // Validate checksum - reject if migration file was modified after apply
                        if stored_checksum != &checksum {
                            return Err(ValkyrinError::Migration(
                                format!("Migration {} has been modified after being applied (checksum mismatch: stored={}, computed={}). Refusing to re-apply.", file_name, stored_checksum, checksum)
                            ));
                        }
                        println!("⏭️  Migration {} already applied (checksum verified)", file_name);
                        continue;
                    }
                    
                    // Execute the migration in a transaction
                    let result = sqlx::query(&sql)
                        .execute(&pool)
                        .await;
                    
                    // Record the result
                    match result {
                        Ok(_) => {
                            println!("✅  Applied migration: {}", file_name);
                            sqlx::query(
                                "INSERT INTO _valkyrin_migrations (version, name, checksum, success) VALUES (?, ?, ?, ?)"
                            )
                            .bind(&version)
                            .bind(file_name)
                            .bind(&checksum)
                            .bind(true)
                            .execute(&pool)
                            .await
                            .map_err(from_sqlx)
                            .map_err(|e| ValkyrinError::Migration(format!("Failed to record migration: {}", e)))?;
                        }
                        Err(e) => {
                            println!("❌  Failed to apply migration: {}", file_name);
                            sqlx::query(
                                "INSERT INTO _valkyrin_migrations (version, name, checksum, success) VALUES (?, ?, ?, ?)"
                            )
                            .bind(&version)
                            .bind(file_name)
                            .bind(&checksum)
                            .bind(false)
                            .execute(&pool)
                            .await
                            .map_err(from_sqlx)
                            .map_err(|e| ValkyrinError::Migration(format!("Failed to record failed migration: {}", e)))?;
                            return Err(ValkyrinError::Migration(format!("Migration failed: {}", e)));
                        }
                    }
                }
                
                Ok(())
            }
            DatabaseType::SQLite => {
                // Create connection pool and acquire lock
                sqlite_lock(db_url).await
                    .map_err(|e| ValkyrinError::Database(format!("Failed to acquire SQLite lock: {}", e)))?;
                let pool = sqlx::Pool::<sqlx::Sqlite>::connect(db_url)
                    .await
                    .map_err(from_sqlx)
                    .map_err(|e| ValkyrinError::Database(format!("Failed to connect to SQLite: {}", e)))?;
                let mut conn = pool.acquire()
                    .await
                    .map_err(from_sqlx)
                    .map_err(|e| ValkyrinError::Database(format!("Failed to acquire SQLite connection: {}", e)))?;
                create_migration_table_sqlite(&mut conn).await
                    .map_err(|e| ValkyrinError::Migration(format!("Failed to create migration table: {}", e)))?;

                // Get already applied migrations from the database (use pool as executor)
                let applied_migrations: Vec<MigrationRecord> = sqlx::query_as(
                    "SELECT version, name, checksum, applied_at, success FROM _valkyrin_migrations"
                )
                .fetch_all(&pool)
                .await
                .map_err(from_sqlx)
                .map_err(|e| ValkyrinError::Introspection(format!("Failed to fetch applied migrations: {}", e)))?;
                
                // Track which migrations have been applied with their checksums
                let mut applied: HashMap<String, String> = HashMap::new();
                for record in applied_migrations {
                    applied.insert(record.version.clone(), record.checksum);
                }
                
                // Execute pending migrations
                for path in candidates {
                    let file_name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
                    let version = file_name.to_string();
                    
                    // Read the migration file first to compute checksum
                    let sql = fs::read_to_string(&path)
                        .map_err(|e| ValkyrinError::Io(format!("Failed to read migration file {}: {}", path.display(), e)))?;
                    let checksum = {
                        let mut hasher = Sha256::new();
                        hasher.update(sql.as_bytes());
                        format!("{:x}", hasher.finalize())
                    };
                    
                    // Check if already applied
                    if let Some(stored_checksum) = applied.get(&version) {
                        // Validate checksum - reject if migration file was modified after apply
                        if stored_checksum != &checksum {
                            return Err(ValkyrinError::Migration(
                                format!("Migration {} has been modified after being applied (checksum mismatch: stored={}, computed={}). Refusing to re-apply.", file_name, stored_checksum, checksum)
                            ));
                        }
                        println!("⏭️  Migration {} already applied (checksum verified)", file_name);
                        continue;
                    }
                    
                    // Execute the migration in a transaction
                    let result = sqlx::query(&sql)
                        .execute(&pool)
                        .await;
                    
                    // Record the result
                    match result {
                        Ok(_) => {
                            println!("✅  Applied migration: {}", file_name);
                            sqlx::query(
                                "INSERT INTO _valkyrin_migrations (version, name, checksum, success) VALUES (?, ?, ?, ?)"
                            )
                            .bind(&version)
                            .bind(file_name)
                            .bind(&checksum)
                            .bind(true)
                            .execute(&pool)
                            .await
                            .map_err(from_sqlx)
                            .map_err(|e| ValkyrinError::Migration(format!("Failed to record migration: {}", e)))?;
                        }
                        Err(e) => {
                            println!("❌  Failed to apply migration: {}", file_name);
                            sqlx::query(
                                "INSERT INTO _valkyrin_migrations (version, name, checksum, success) VALUES (?, ?, ?, ?)"
                            )
                            .bind(&version)
                            .bind(file_name)
                            .bind(&checksum)
                            .bind(false)
                            .execute(&pool)
                            .await
                            .map_err(from_sqlx)
                            .map_err(|e| ValkyrinError::Migration(format!("Failed to record failed migration: {}", e)))?;
                            return Err(ValkyrinError::Migration(format!("Migration failed: {}", e)));
                        }
                    }
                }
                
                Ok(())
            }
        }
    }

    /// Rolls back the last N migrations by executing their DOWN SQL.
    pub async fn rollback_migrations(
        db_url: &str,
        explicit_db_type: Option<&str>,
        steps: usize,
        dry_run: bool,
    ) -> ValkyrinResult<()> {
        // Determine DB type
        let db_type = if let Some(db_type_str) = explicit_db_type {
            match db_type_str.to_lowercase().as_str() {
                "postgres" | "postgresql" => DatabaseType::PostgreSQL,
                "mysql" => DatabaseType::MySQL,
                "sqlite" => DatabaseType::SQLite,
                _ => {
                    return Err(ValkyrinError::Config(
                        format!("Unknown database type: '{}'. Use 'postgres', 'mysql', or 'sqlite'.", db_type_str)
                    ))
                }
            }
        } else {
            DatabaseType::from_url(db_url)?
        };

        // Find migration files for the DB type
        let suffix = match db_type {
            DatabaseType::PostgreSQL => "postgres",
            DatabaseType::MySQL => "mysql",
            DatabaseType::SQLite => "sqlite",
        };
        let migration_dir = std::path::Path::new("migrations");
        let mut candidates: Vec<std::path::PathBuf> = fs::read_dir(migration_dir)
            .map_err(|e| ValkyrinError::Io(format!("Failed to read migrations directory: {}", e)))?
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .filter(|p| {
                p.extension()
                    .and_then(|ext| ext.to_str())
                    .map(|ext| ext.eq_ignore_ascii_case("sql"))
                    .unwrap_or(false)
            })
            .filter(|p| {
                p.file_name()
                    .and_then(|n| n.to_str())
                    .map(|n| n.contains(suffix))
                    .unwrap_or(false)
            })
            .collect();

        // Sort by filename (timestamp part) descending (newest first)
        candidates.sort_by(|a, b| b.cmp(a));

        if candidates.is_empty() {
            println!("⚠️  No migration files found for {}", suffix);
            return Ok(());
        }

        // Take the last N migrations to rollback
        let to_rollback = candidates.into_iter().take(steps).collect::<Vec<_>>();

        for path in to_rollback {
            println!("⏪ Rolling back migration: {}", path.display());
            let content = fs::read_to_string(&path)
                .map_err(|e| ValkyrinError::Io(format!("Failed to read migration file {}: {}", path.display(), e)))?;

            // Parse the migration file to extract DOWN SQL
            // Migration files have format: -- DOWN SQL at the end or separate file
            // For now, we'll look for a corresponding .down.sql file or parse -- DOWN: comments
            let down_path = path.with_extension("down.sql");
            let down_sql = if down_path.exists() {
                fs::read_to_string(&down_path)
                    .map_err(|e| ValkyrinError::Io(format!("Failed to read down migration file {}: {}", down_path.display(), e)))?
            } else {
                // Try to extract DOWN SQL from comments in the migration file
                let mut down_stmts = Vec::new();
                let mut in_down_section = false;
                for line in content.lines() {
                    if line.trim() == "-- DOWN" {
                        in_down_section = true;
                        continue;
                    }
                    if in_down_section {
                        if line.trim().starts_with("-- UP") || line.trim().starts_with("--") {
                            break;
                        }
                        down_stmts.push(line);
                    }
                }
                down_stmts.join("\n")
            };

            if down_sql.trim().is_empty() {
                println!("   ⚠️  No DOWN SQL found for {}, skipping", path.display());
                continue;
            }

            if dry_run {
                println!("   [DRY-RUN] Would execute DOWN migration:");
                for stmt in down_sql.split(';') {
                    let trimmed = stmt.trim();
                    if !trimmed.is_empty() && !trimmed.starts_with("--") {
                        println!("     ▶ {};", trimmed);
                    }
                }
            } else {
                let mut statements = Vec::new();
                for stmt in down_sql.split(';') {
                    let trimmed = stmt.trim();
                    if trimmed.is_empty() || trimmed.starts_with("--") {
                        continue;
                    }
                    statements.push(format!("{};", trimmed));
                }
                if !statements.is_empty() {
                    println!("📜 Executing {} DOWN statements:", statements.len());
                    for s in &statements {
                        println!("   ▶ {}", s);
                    }
                    Self::execute_migration(db_url, db_type, &statements).await?;
                    println!("✅ Rolled back: {}", path.display());
                }
            }
        }

        Ok(())
    }

    /// Pushes canvas changes to the database, optionally applying destructive changes.
    pub async fn push_to_database(
        db_url: &str,
        explicit_db_type: Option<&str>,
        confirm: bool,
        dry_run: bool,
    ) -> ValkyrinResult<()> {
        // Determine DB type
        let db_type = if let Some(db_type_str) = explicit_db_type {
            match db_type_str.to_lowercase().as_str() {
                "postgres" | "postgresql" => DatabaseType::PostgreSQL,
                "mysql" => DatabaseType::MySQL,
                "sqlite" => DatabaseType::SQLite,
                _ => {
                    return Err(ValkyrinError::Config(
                        format!("Unknown database type: '{}'. Use 'postgres', 'mysql', or 'sqlite'.", db_type_str)
                    ))
                }
            }
        } else {
            DatabaseType::from_url(db_url)?
        };

        println!("🔍 Introspecting live {} schema...", match db_type {
            DatabaseType::PostgreSQL => "PostgreSQL",
            DatabaseType::MySQL => "MySQL",
            DatabaseType::SQLite => "SQLite",
        });

        // Introspect live schema
        let (live_schema, _detected_relations) = match db_type {
            DatabaseType::PostgreSQL => {
                let introspector = PostgresIntrospector::new(db_url).await?;
                let schema = introspector.fetch_schema().await?;
                let relations = introspector.fetch_relations().await?;
                (schema, relations)
            }
            DatabaseType::MySQL => {
                let introspector = MysqlIntrospector::new(db_url).await?;
                let schema = introspector.fetch_schema().await?;
                let relations = introspector.fetch_relations().await?;
                (schema, relations)
            }
            DatabaseType::SQLite => {
                let introspector = SqliteIntrospector::new(db_url).await?;
                let schema = introspector.fetch_schema().await?;
                let relations = introspector.fetch_relations().await?;
                (schema, relations)
            }
        };

        // Load canvas
        let local_file = fs::read_to_string("schema.vdb.json")
            .unwrap_or_else(|_| r#"{"tables":[],"relations":[]}"#.to_string());
        let payload: crate::canvas::CanvasPayload =
            serde_json::from_str(&local_file)
                .map_err(|e| ValkyrinError::Config(format!("Failed to parse local schema.vdb.json: {}", e)))?;
        let canvas_ir = payload.to_ir();

        // Compute diff from canvas to DB (swap arguments)
        let diff = Self::calculate_detailed_diff(&canvas_ir.entities, &live_schema);

        // Generate migration statements (forward direction: DB -> canvas)
        // For push we need opposite: apply canvas => DB changes.
        let mut statements = Vec::new();

        // DROP tables that exist in DB but not in canvas (destructive)
        for tbl in &diff.removed_tables {
            if confirm {
                statements.push(format!("DROP TABLE IF EXISTS \"{}\";", tbl));
            } else {
                println!("⚠️  Table '{}' would be dropped. Use --confirm to apply.", tbl);
            }
        }

        // CREATE tables that are in canvas but not in DB
        for entity in diff.new_tables.iter() {
            let cols: Vec<String> = entity
                .fields
                .iter()
                .map(|f| {
                    let sql_type = Self::ir_type_to_sql(&f.data_type, db_type);
                    let mut parts = vec![format!("    \"{}\" {}", f.name, sql_type)];
                    if f.constraints.is_primary_key {
                        parts.push("PRIMARY KEY".to_string());
                    }
                    if !f.constraints.is_nullable {
                        parts.push("NOT NULL".to_string());
                    }
                    if f.constraints.is_unique {
                        parts.push("UNIQUE".to_string());
                    }
                    if let Some(ref def) = f.constraints.default_value {
                        parts.push(format!("DEFAULT {}", def));
                    }
                    parts.join(" ")
                })
                .collect();
            statements.push(format!(
                "CREATE TABLE \"{}\" (\n{},\n    \"id\" UUID PRIMARY KEY\n);",
                entity.name,
                cols.join(",\n")
            ));
        }

        // ALTER TABLE for column additions and removals (destructive removals need confirm)
        for td in &diff.modified_tables {
            // Columns to add (present in canvas but not DB) – these are in td.adds when we swapped arguments
            for f in &td.adds {
                let sql_type = Self::ir_type_to_sql(&f.data_type, db_type);
                let nullable = if f.constraints.is_nullable { "" } else { " NOT NULL" };
                statements.push(format!(
                    "ALTER TABLE \"{}\" ADD COLUMN \"{}\" {}{};",
                    td.table_name, f.name, sql_type, nullable
                ));
            }
            // Columns to drop (present in DB but not canvas) – these are in td.removes
            for f in &td.removes {
                if confirm {
                    statements.push(format!(
                        "ALTER TABLE \"{}\" DROP COLUMN \"{}\";",
                        td.table_name, f.name
                    ));
                } else {
                    println!(
                        "⚠️  Column '{}' in table '{}' would be dropped. Use --confirm to apply.",
                        f.name, td.table_name
                    );
                }
            }
        }

        if statements.is_empty() {
            println!("✅ No changes detected between canvas and database.");
            return Ok(());
        }

        // Show migration plan
        println!("\n📝 Migration plan ({} statements):", statements.len());
        for s in &statements {
            println!("   {}", s);
        }

        if dry_run {
            println!("🏁 Dry-run complete. No changes applied.");
            return Ok(());
        }

        // Execute statements
        Self::execute_migration(db_url, db_type, &statements).await
    }

    /// Checks synchronization status between canvas and database (dry-run diff).
    pub async fn check_sync(db_url: &str, explicit_db_type: Option<&str>) -> ValkyrinResult<()> {
        // Reuse synchronize_database in DryRun mode but suppress file writes
        Self::synchronize_database(db_url, explicit_db_type, SyncMode::DryRun).await
    }

    /// Connects to a database, diffs the live schema against the local canvas,
    /// prints a detailed diff report, and optionally applies changes.
    pub async fn synchronize_database(
        db_url: &str,
        explicit_db_type: Option<&str>,
        mode: SyncMode,
    ) -> ValkyrinResult<()> {
        println!("🔌 Connecting to database...");

        let db_type = if let Some(db_type_str) = explicit_db_type {
            match db_type_str.to_lowercase().as_str() {
                "postgres" | "postgresql" => DatabaseType::PostgreSQL,
                "mysql" => DatabaseType::MySQL,
                "sqlite" => DatabaseType::SQLite,
                _ => {
                    return Err(ValkyrinError::Config(
                        format!("Unknown database type: '{}'. Use 'postgres', 'mysql', or 'sqlite'.", db_type_str)
                    ))
                }
            }
        } else {
            DatabaseType::from_url(db_url)?
        };

        let db_label = match db_type {
            DatabaseType::PostgreSQL => "PostgreSQL",
            DatabaseType::MySQL => "MySQL",
            DatabaseType::SQLite => "SQLite",
        };
        println!("🔍 Introspecting live {} schema...", db_label);

        // Introspect schema + relations
        let (live_schema, detected_relations) = match db_type {
            DatabaseType::PostgreSQL => {
                let introspector = PostgresIntrospector::new(db_url).await?;
                let schema = introspector.fetch_schema().await?;
                let relations = introspector.fetch_relations().await?;
                (schema, relations)
            }
            DatabaseType::MySQL => {
                let introspector = MysqlIntrospector::new(db_url).await?;
                let schema = introspector.fetch_schema().await?;
                let relations = introspector.fetch_relations().await?;
                (schema, relations)
            }
            DatabaseType::SQLite => {
                let introspector = SqliteIntrospector::new(db_url).await?;
                let schema = introspector.fetch_schema().await?;
                let relations = introspector.fetch_relations().await?;
                (schema, relations)
            }
        };

        println!(
            "   Found {} table(s), {} FK relation(s) in live database.",
            live_schema.len(),
            detected_relations.len()
        );

        // Load the local canvas
        let local_file = fs::read_to_string("schema.vdb.json")
            .unwrap_or_else(|_| r#"{"tables":[],"relations":[]}"#.to_string());
        let mut payload: crate::canvas::CanvasPayload =
            serde_json::from_str(&local_file)
                .map_err(|e| ValkyrinError::Config(format!("Failed to parse local schema.vdb.json: {}", e)))?;

        let local_ir = payload.to_ir();

        // Compute detailed diff
        let mut diff = Self::calculate_detailed_diff(&live_schema, &local_ir.entities);
        diff.detected_relations = detected_relations;

        // Print diff report
        println!("\n{}", "─".repeat(60));
        println!("   📊 Bidirectional Diff Report");
        println!("{}", "─".repeat(60));
        print!("{}", Self::format_diff_report(&diff));

        // Print detected relations
        if !diff.detected_relations.is_empty() {
            println!("\n   🔗 Detected Foreign Key Relations:");
            for rel in &diff.detected_relations {
                println!(
                    "      {} ({}) → {} ({})",
                    rel.source_table, rel.source_column, rel.target_table, rel.target_column
                );
            }
        }

        // Generate and show migration SQL
        let migration_stmts = Self::generate_migration(&local_ir.entities, &diff, db_type);
        if !migration_stmts.is_empty() {
            println!("\n   📝 Suggested Migration SQL:");
            for stmt in &migration_stmts {
                println!("      {}", stmt);
            }
            // Write migration file (except in dry-run mode)
            if mode != SyncMode::DryRun {
                Self::write_migration_file(&local_ir.entities, &diff, &migration_stmts, db_type)?;
            }
        }

        println!("{}", "─".repeat(60));

        // In dry-run mode, stop here
        if mode == SyncMode::DryRun {
            println!("\n🏁 Dry-run complete. No files were modified.");
            return Ok(());
        }

        let has_new_tables = !diff.new_tables.is_empty();
        let has_removed = !diff.removed_tables.is_empty();

        // If there's nothing to do, exit
        if !has_new_tables && !has_removed && diff.modified_tables.is_empty() {
            println!("✅ Canvas is already perfectly synced with the live database.");
            return Ok(());
        }

        // Apply changes
        if has_new_tables {
            println!("\n🚀 Injecting new tables into canvas...");
            let existing_positions: Vec<(f32, f32)> = payload
                .tables
                .iter()
                .map(|t| (t.position.x, t.position.y))
                .collect();
            let mut next_spawn = Self::calculate_safe_spawn_point(&existing_positions);

            for new_table in &diff.new_tables {
                let mut canvas_columns = Vec::new();
                for field in &new_table.fields {
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
                        DataType::Enum { values: _, type_name: _ } => "enum",
                    };

                    canvas_columns.push(crate::canvas::CanvasColumn {
                        id: field.id.clone(),
                        name: field.name.clone(),
                        raw_type: type_str.to_string(),
                        is_primary: field.constraints.is_primary_key,
                        is_nullable: field.constraints.is_nullable,
                        is_unique: field.constraints.is_unique,
                        is_indexed: field.constraints.is_indexed,
                        default_value: field.constraints.default_value.clone(),
                        enum_values: match &field.data_type {
                            DataType::Enum { values, .. } => Some(values.clone()),
                            _ => None,
                        },
                        enum_type_name: match &field.data_type {
                            DataType::Enum { type_name, .. } => type_name.clone(),
                            _ => None,
                        },
                        precision: match &field.data_type {
                            DataType::Decimal { precision, .. } => Some(*precision),
                            _ => None,
                        },
                        scale: match &field.data_type {
                            DataType::Decimal { scale, .. } => Some(*scale),
                            _ => None,
                        },
                        max_length: match &field.data_type {
                            DataType::String { max_length } => *max_length,
                            _ => None,
                        },
                    });
                }

                // UUID collision check: ensure new_table.id doesn't exist in canvas
                let mut table_id = new_table.id.clone();
                let existing_ids: std::collections::HashSet<String> = payload.tables.iter().map(|t| t.id.clone()).collect();
                if existing_ids.contains(&table_id) {
                    // Generate one new UUID
                    table_id = uuid::Uuid::new_v4().to_string();
                    if existing_ids.contains(&table_id) {
                        panic!("UUID collision detected twice - this should be statistically impossible");
                    }
                }

                payload.tables.push(crate::canvas::CanvasTable {
                    id: table_id,
                    name: new_table.name.clone(),
                    columns: canvas_columns,
                    position: crate::canvas::NodePosition {
                        x: next_spawn.0,
                        y: next_spawn.1,
                    },
                });

                println!("   ✨ Injected: {}", new_table.name);
                next_spawn.1 += 250.0;
            }
        }

        // Handle destructive changes (require mode == ApplyAll)
        if has_removed {
            if mode == SyncMode::ApplyAll {
                println!("\n🗑️  Removing tables from canvas...");
                payload
                    .tables
                    .retain(|t| !diff.removed_tables.contains(&t.name));
                payload.relations.retain(|r| {
                    let source_kept = payload.tables.iter().any(|t| t.id == r.source_table_id);
                    let target_kept = payload.tables.iter().any(|t| t.id == r.target_table_id);
                    source_kept && target_kept
                });
                for name in &diff.removed_tables {
                    println!("   🗑️  Removed: {}", name);
                }
            } else {
                println!("\n   ⚠️  Removed tables were NOT applied. Use --confirm to remove them.");
            }
        }

        // Inject detected FK relations
        if !diff.detected_relations.is_empty() {
            for rel in &diff.detected_relations {
                let source_id = payload
                    .tables
                    .iter()
                    .find(|t| t.name == rel.source_table)
                    .map(|t| t.id.clone());
                let target_id = payload
                    .tables
                    .iter()
                    .find(|t| t.name == rel.target_table)
                    .map(|t| t.id.clone());

                if let (Some(sid), Some(tid)) = (source_id, target_id) {
                    let already_exists = payload
                        .relations
                        .iter()
                        .any(|r| r.source_table_id == sid && r.target_table_id == tid);
                    if !already_exists {
                        payload.relations.push(crate::canvas::CanvasRelation {
                            id: uuid::Uuid::new_v4().to_string(),
                            source_table_id: sid,
                            target_table_id: tid,
                            relation_type: "1:N".to_string(),
                        });
                    }
                }
            }
        }

        // Write the updated blueprint
        let pretty_json = serde_json::to_string_pretty(&payload)?;
        fs::write("schema.vdb.json", pretty_json)?;
        println!("\n💾 Canvas blueprint updated successfully!");
        println!("   Boot the canvas to see the changes.");

        Ok(())
    }
}

/// Extension trait for TableDiff to check if it has any changes
trait HasChanges {
    fn has_changes(&self) -> bool;
}

impl HasChanges for TableDiff {
    fn has_changes(&self) -> bool {
        !self.adds.is_empty() || !self.removes.is_empty() || !self.changes.is_empty()
    }
}
