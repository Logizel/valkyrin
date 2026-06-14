// valkyrin-core/src/sync.rs
use crate::ir::{DataType, Entity, Field};
use sqlx::{Pool, Postgres};

/// The universal contract for reading a live database.
#[async_trait::async_trait]
pub trait DatabaseIntrospector {
    /// Connects to the database and extracts the physical table structures.
    async fn fetch_schema(&self, pool: &Pool<Postgres>) -> Result<Vec<Entity>, sqlx::Error>;
}
use sqlx::Row;

pub struct PostgresIntrospector;

#[async_trait::async_trait]
impl DatabaseIntrospector for PostgresIntrospector {
    async fn fetch_schema(&self, pool: &Pool<Postgres>) -> Result<Vec<Entity>, sqlx::Error> {
        // Query the internal PostgreSQL catalog for all tables and columns
        let query = r#"
            SELECT 
                table_name, 
                column_name, 
                data_type, 
                is_nullable 
            FROM information_schema.columns 
            WHERE table_schema = 'public'
            ORDER BY table_name, ordinal_position;
        "#;

        let rows = sqlx::query(query).fetch_all(pool).await?;
        let mut entities: Vec<Entity> = Vec::new();
        let mut current_table_name = String::new();
        let mut current_fields: Vec<Field> = Vec::new();

        for row in rows {
            let table_name: String = row.get("table_name");
            let column_name: String = row.get("column_name");
            let db_type: String = row.get("data_type");
            let is_nullable_str: String = row.get("is_nullable");

            // Push the previous table into our IR memory map when the table name changes
            if table_name != current_table_name && !current_table_name.is_empty() {
                entities.push(Entity {
                    id: uuid::Uuid::new_v4().to_string(),
                    name: current_table_name.clone(),
                    fields: current_fields.clone(),
                });
                current_fields.clear();
            }

            current_table_name = table_name;

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
                _ => DataType::Text, // Safe fallback
            };

            current_fields.push(Field {
                id: uuid::Uuid::new_v4().to_string(),
                name: column_name,
                data_type: mapped_type,
                constraints: crate::ir::Constraints {
                    is_primary_key: false, // In a full production build, this requires joining the pg_constraint table
                    is_unique: false,
                    is_nullable: is_nullable_str == "YES",
                    is_indexed: false,
                    default_value: None,
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

pub struct SyncEngine;

pub struct SchemaDiff {
    pub new_tables: Vec<Entity>,
}

impl SyncEngine {
    /// Compares the live database state against the local canvas state.
    pub fn calculate_diff(live_schema: &[Entity], local_schema: &[Entity]) -> SchemaDiff {
        let mut diff = SchemaDiff {
            new_tables: Vec::new(),
        };

        for live_table in live_schema {
            let found_locally = local_schema.iter().any(|loc| loc.name == live_table.name);

            if !found_locally {
                diff.new_tables.push(live_table.clone());
            }
        }

        diff
    }

    /// Calculates a safe X/Y coordinate for a new table so it does not overlap existing tables.
    pub fn calculate_safe_spawn_point(existing_layout: &[(f32, f32)]) -> (f32, f32) {
        if existing_layout.is_empty() {
            return (100.0, 100.0);
        }

        // Find the furthest table on the X-axis
        let mut max_x = 0.0;
        let mut base_y = 100.0;

        for (x, y) in existing_layout {
            if *x > max_x {
                max_x = *x;
                base_y = *y; // Align it horizontally with the furthest table
            }
        }

        // Spawn 300 pixels to the right of the furthest table
        (max_x + 300.0, base_y)
    }
} // valkyrin-core/src/sync.rs
// (Leave your existing trait, structs, and implementations intact at the top)

use crate::canvas::{CanvasColumn, CanvasPayload, CanvasTable, NodePosition};
use anyhow::{Context, Result as AnyhowResult};
use std::fs;

// Rust allows multiple impl blocks for the same struct, so we can cleanly append this:
impl SyncEngine {
    /// Connects to PostgreSQL, diffs the live catalog against the local canvas, and updates the JSON layout.
    pub async fn synchronize_database(db_url: &str) -> AnyhowResult<()> {
        // 1. Connect to the database
        println!("🔌 Connecting to database...");
        let pool = Pool::<Postgres>::connect(db_url)
            .await
            .context("Failed to connect to PostgreSQL. Is the URL correct?")?;

        // 2. Fetch the live PostgreSQL catalog bypassing the ORM
        println!("🔍 Introspecting live schema...");
        let introspector = PostgresIntrospector;
        let live_schema = introspector
            .fetch_schema(&pool)
            .await
            .context("Failed to read PostgreSQL catalog. Check permissions.")?;

        // 3. Load the local canvas layout
        let local_file = fs::read_to_string("schema.vdb.json")
            .unwrap_or_else(|_| r#"{"tables":[],"relations":[]}"#.to_string());
        let mut payload: CanvasPayload =
            serde_json::from_str(&local_file).context("Failed to parse local schema.vdb.json")?;

        // 4. Diff the schemas
        let local_ir = payload.to_ir();
        let diff = Self::calculate_diff(&live_schema, &local_ir.entities);

        if diff.new_tables.is_empty() {
            println!("✅ Canvas is already perfectly synced with the live database.");
            return Ok(());
        }

        // 5. Safely inject new tables into the visual layout
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
                    DataType::Integer(_) => "int",
                    DataType::Boolean => "boolean",
                    DataType::DateTime => "datetime",
                    DataType::Json => "json",
                    DataType::Uuid => "uuid",
                    _ => "string",
                };

                canvas_columns.push(CanvasColumn {
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

            payload.tables.push(CanvasTable {
                id: new_table.id,
                name: new_table.name,
                columns: canvas_columns,
                position: NodePosition {
                    x: next_spawn.0,
                    y: next_spawn.1,
                },
            });

            // Move the next spawn point down to prevent UI nodes from stacking on top of each other
            next_spawn.1 += 250.0;
        }

        // 6. Write the updated blueprint back to disk
        let pretty_json = serde_json::to_string_pretty(&payload)?;
        fs::write("schema.vdb.json", pretty_json)?;
        println!("💾 Canvas blueprint updated! Boot the canvas to see the new tables.");

        Ok(())
    }
}
