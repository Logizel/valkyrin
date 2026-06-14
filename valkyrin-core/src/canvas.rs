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
    pub position: NodePosition,
}
#[derive(Serialize, Deserialize, Debug)]
pub struct NodePosition {
    pub x: f32,
    pub y: f32,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CanvasColumn {
    pub id: String,
    pub name: String,
    pub raw_type: String, // e.g., "string", "int", "float", "text", "decimal", "bigint", "smallint"
    pub is_primary: bool,
    pub is_nullable: bool,
    pub is_unique: bool,
    pub is_indexed: bool,
    pub default_value: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CanvasRelation {
    pub id: String,
    pub source_table_id: String,
    pub target_table_id: String,
    pub relation_type: String, // "1:1", "1:N", "M:N"
}

use crate::ir::{
    Connection, Constraints, DataType, Entity, EntityGraph, Field, IntSize, RelationType,
};

// ... (Keep your struct definitions exactly as they are)

impl CanvasPayload {
    pub fn to_ir(&self) -> EntityGraph {
        let mut entities = Vec::new();

        for table in &self.tables {
            let mut fields = Vec::new();

            for col in &table.columns {
                let data_type = match col.raw_type.as_str() {
                    "string" => DataType::String { max_length: None },
                    "text" => DataType::Text,
                    "int" | "integer" => DataType::Integer(IntSize::Standard),
                    "bigint" => DataType::Integer(IntSize::Big),
                    "smallint" | "int16" => DataType::Integer(IntSize::Small),
                    "float" => DataType::Float,
                    "decimal" => {
                        // Default to (10, 2) if not specified; ideally parsed from raw_type
                        DataType::Decimal {
                            precision: 10,
                            scale: 2,
                        }
                    }
                    "boolean" | "bool" => DataType::Boolean,
                    "datetime" | "timestamp" => DataType::DateTime,
                    "json" | "jsonb" => DataType::Json,
                    "uuid" => DataType::Uuid,
                    _ => DataType::Text, // Safe fallback
                };

                fields.push(Field {
                    id: col.id.clone(),
                    name: col.name.clone(),
                    data_type,
                    constraints: Constraints {
                        is_primary_key: col.is_primary,
                        is_nullable: col.is_nullable,
                        is_unique: col.is_unique,
                        is_indexed: col.is_indexed,
                        default_value: col.default_value.clone(),
                    },
                });
            }

            entities.push(Entity {
                id: table.id.clone(),
                name: table.name.clone(),
                fields,
            });
        }

        // NEW: Parse the relationships from the canvas
        let mut connections = Vec::new();
        for rel in &self.relations {
            let multiplicity = match rel.relation_type.as_str() {
                "1:1" => RelationType::OneToOne,
                "M:N" | "N:M" => RelationType::ManyToMany,
                _ => RelationType::OneToMany, // Default 1:N
            };

            connections.push(Connection {
                source_entity_id: rel.source_table_id.clone(),
                target_entity_id: rel.target_table_id.clone(),
                multiplicity,
            });
        }

        EntityGraph {
            entities,
            connections,
        }
    }
}
