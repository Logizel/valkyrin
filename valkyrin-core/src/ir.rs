// valkyrin-core/src/ir.rs

/// Represents the universal data types Valkyrin understands.
#[derive(Debug, Clone, PartialEq)]
pub enum DataType {
    String { max_length: Option<u32> },
    Text,
    Integer(IntSize),
    Float,
    Decimal { precision: u8, scale: u8 },
    Boolean,
    DateTime,
    Json,
    Uuid,
    Enum { values: Vec<String>, type_name: Option<String> },
}

/// Defines the memory size footprint of an integer.
#[derive(Debug, Clone, PartialEq)]
pub enum IntSize {
    Small,    // e.g., INT2 / int16
    Standard, // e.g., INT4 / int32
    Big,      // e.g., INT8 / int64
}

/// Represents the strict multiplicity of a relationship.
#[derive(Debug, Clone, PartialEq)]
pub enum RelationType {
    OneToOne,
    OneToMany,
    ManyToMany,
}

pub struct EntityGraph {
    pub entities: Vec<Entity>,
    pub connections: Vec<Connection>,
    pub relations: Vec<Relation>,
}

#[derive(Debug, Clone)]
pub struct Entity {
    pub id: String,
    pub name: String,
    pub fields: Vec<Field>,
}

#[derive(Debug, Clone)]
pub struct Field {
    pub id: String,
    pub name: String,
    pub data_type: DataType,
    pub constraints: Constraints,
}

#[derive(Debug, Clone)]
pub struct Constraints {
    pub is_primary_key: bool,
    pub primary_key_order: Option<usize>,
    pub is_unique: bool,
    pub is_nullable: bool,
    pub is_indexed: bool,
    pub default_value: Option<String>,
}

#[derive(Debug, Clone)]
pub struct Connection {
    pub source_entity_id: String,
    pub target_entity_id: String,
    pub multiplicity: RelationType,
}

/// Represents a foreign key relationship between two entities.
#[derive(Debug, Clone)]
pub struct Relation {
    pub id: String,
    pub name: String,
    pub source_entity_id: String,
    pub source_field_name: String,
    pub target_entity_id: String,
    pub target_field_name: String,
    pub relation_type: RelationType,
    pub on_delete: Option<ReferentialAction>,
    pub on_update: Option<ReferentialAction>,
}

/// Referential actions for foreign keys.
#[derive(Debug, Clone, PartialEq)]
pub enum ReferentialAction {
    Cascade,
    Restrict,
    SetNull,
    NoAction,
    SetDefault,
}
