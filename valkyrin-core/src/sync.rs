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
                "integer" | "bigint" => DataType::Integer(crate::ir::IntSize::Standard),
                "boolean" => DataType::Boolean,
                "timestamp without time zone" => DataType::DateTime,
                "jsonb" => DataType::Json,
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
}
