// valkyrin-core/src/canvas.rs
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct CanvasPayload {
    pub tables: Vec<CanvasTable>,
    pub relations: Vec<CanvasRelation>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CanvasTable {
    pub id: String, // The immutable UUID
    pub name: String,
    pub columns: Vec<CanvasColumn>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CanvasColumn {
    pub id: String,
    pub name: String,
    pub raw_type: String, // e.g., "string", "int", "boolean" from the UI dropdown
    pub is_primary: bool,
    pub is_nullable: bool,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CanvasRelation {
    pub id: String,
    pub source_table_id: String,
    pub target_table_id: String,
    pub relation_type: String, // "1:1", "1:N", "M:N"
}

use crate::ir::{Constraints, DataType, Entity, EntityGraph, Field, IntSize};

impl CanvasPayload {
    /// Transforms the raw visual UI data into the strict Rust compiler memory map.
    pub fn to_ir(&self) -> EntityGraph {
        let mut entities = Vec::new();

        for table in &self.tables {
            let mut fields = Vec::new();

            for col in &table.columns {
                // Map frontend string types to safe Rust Enums
                let data_type = match col.raw_type.as_str() {
                    "string" => DataType::String { max_length: None },
                    "int" | "integer" => DataType::Integer(IntSize::Standard),
                    "boolean" | "bool" => DataType::Boolean,
                    "datetime" => DataType::DateTime,
                    "json" => DataType::Json,
                    "uuid" => DataType::Uuid,
                    _ => DataType::Text, // Fallback
                };

                fields.push(Field {
                    id: col.id.clone(),
                    name: col.name.clone(),
                    data_type,
                    constraints: Constraints {
                        is_primary_key: col.is_primary,
                        is_nullable: col.is_nullable,
                        is_unique: false,
                    },
                });
            }

            entities.push(Entity {
                id: table.id.clone(),
                name: table.name.clone(), // e.g., "Users Table"
                fields,
            });
        }

        EntityGraph {
            entities,
            connections: vec![], // Relations logic will be mapped here later
        }
    }
}
