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
