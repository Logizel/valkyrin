// valkyrin-core/src/codegen.rs
use crate::ir::{DataType, Entity, EntityGraph, Field, IntSize, Relation, RelationType, ReferentialAction};
use crate::error::ValkyrinResult;
use anyhow::Result;

/// The universal contract for code generation.
pub trait LanguageDriver {
    /// Translates a universal Valkyrin type into the language-specific type.
    fn map_data_type(&self, data_type: &DataType, is_nullable: bool) -> String;

    /// Compiles a single entity into a complete file with imports and struct/class definition.
    fn generate_model(&self, entity: &Entity) -> String;

    /// Returns the file extension for this language (e.g., "go", "py", "rs").
    fn file_extension(&self) -> &'static str;

    /// Generates relation code for an entity (belongs_to, has_many, etc.)
    fn generate_relations(&self, entity: &Entity, relations: &[Relation]) -> String {
        let _ = entity;
        let _ = relations;
        String::new()
    }

    /// Generates a complete client library for the given entity graph.
    /// Returns an optional list of (filename, content) tuples.
    fn generate_full_client(&self, _graph: &EntityGraph) -> ValkyrinResult<Option<Vec<(String, String)>>> {
        Ok(None)
    }

    /// Maps a referential action to the target language syntax.
    fn map_referential_action(&self, action: &ReferentialAction) -> String {
        match action {
            ReferentialAction::Cascade => "CASCADE".to_string(),
            ReferentialAction::Restrict => "RESTRICT".to_string(),
            ReferentialAction::SetNull => "SET NULL".to_string(),
            ReferentialAction::NoAction => "NO ACTION".to_string(),
            ReferentialAction::SetDefault => "SET DEFAULT".to_string(),
        }
    }

    /// Generates enums file (TypeScriptValkyrin only)
    fn generate_enums(&self, _graph: &EntityGraph) -> String {
        String::new()
    }

    /// Generates types file with type-level machinery (TypeScriptValkyrin only)
    fn generate_types(&self) -> String {
        String::new()
    }

    /// Generates operations file with Select/Include/Omit/Args types (TypeScriptValkyrin only)
    fn generate_operations(&self, _graph: &EntityGraph) -> String {
        String::new()
    }

    /// Generates client runtime file (TypeScriptValkyrin only)
    fn generate_client(&self, _graph: &EntityGraph) -> String {
        String::new()
    }

    /// Generates index.ts barrel export (TypeScriptValkyrin only)
    fn generate_index(&self, _graph: &EntityGraph) -> String {
        String::new()
    }
}

pub struct GoGormDriver;

impl LanguageDriver for GoGormDriver {
    fn map_data_type(&self, data_type: &DataType, is_nullable: bool) -> String {
        let base_type = match data_type {
            DataType::String { .. } | DataType::Text => "string",
            DataType::Integer(crate::ir::IntSize::Small) => "int16",
            DataType::Integer(crate::ir::IntSize::Standard) => "int",
            DataType::Integer(crate::ir::IntSize::Big) => "int64",
            DataType::Float => "float64",
            DataType::Decimal { .. } => "decimal.Decimal", // Use shopspring/decimal
            DataType::Boolean => "bool",
            DataType::DateTime => "time.Time",
            DataType::Json => "datatypes.JSON",
            DataType::Uuid => "uuid.UUID",
            DataType::Enum { values: _, type_name } => {
                // Use native PostgreSQL enum type name if available
                type_name.as_deref().unwrap_or("string")
            }
        };

        if is_nullable {
            format!("*{}", base_type)
        } else {
            base_type.to_string()
        }
    }

    fn file_extension(&self) -> &'static str {
        "go"
    }

    fn generate_model(&self, entity: &Entity) -> String {
        let mut output = String::new();

        output.push_str("package models\n\n");

        let mut imports = vec!["\"time\"", "\"github.com/shopspring/decimal\""];

        let has_json = entity
            .fields
            .iter()
            .any(|f| matches!(f.data_type, DataType::Json));
        if has_json {
            imports.push("\"gorm.io/datatypes\"");
        }

        let has_uuid = entity
            .fields
            .iter()
            .any(|f| matches!(f.data_type, DataType::Uuid));
        if has_uuid {
            imports.push("\"github.com/google/uuid\"");
        }

        output.push_str(&format!("import (\n\t{}\n)\n\n", imports.join("\n\t")));

        // Generate enum constants for each enum field (fallback for non-PostgreSQL enums)
        // For PostgreSQL native enums (with type_name), we just need the type declaration
        let enum_fields: Vec<&Field> = entity
            .fields
            .iter()
            .filter(|f| matches!(f.data_type, DataType::Enum { values: _, type_name: _ }))
            .collect();

        if !enum_fields.is_empty() {
            for field in &enum_fields {
                if let DataType::Enum { values, type_name } = &field.data_type {
                    if let Some(enum_type) = type_name {
                        // Native PostgreSQL enum - emit CREATE TYPE (handled in migrations)
                        // Go will use the type name directly
                        output.push_str(&format!("type {} = string\n\n", enum_type));
                    } else {
                        // Fallback: generate Go constants
                        let const_name = format!("{}Status", capitalize_first(&field.name));
                        output.push_str(&format!("type {} string\n\n", const_name));
                        output.push_str("const (\n");
                        for val in values {
                            let const_val = format!("{}{}", const_name, capitalize_first(val));
                            output.push_str(&format!("\t{} {} = \"{}\"\n", const_val, const_name, val));
                        }
                        output.push_str(")\n\n");
                    }
                }
            }
        }

        output.push_str(&format!("type {} struct {{\n", entity.name));

        let pk_fields: Vec<&Field> = entity
            .fields
            .iter()
            .filter(|f| f.constraints.is_primary_key)
            .collect();
        let _has_composite_pk = pk_fields.len() > 1;

        for field in &entity.fields {
            let go_type = self.map_data_type(&field.data_type, field.constraints.is_nullable);

            let mut gorm_tags = vec![format!("column:{}", field.name)];
            if field.constraints.is_primary_key {
                gorm_tags.push("primaryKey".to_string());
            }
            if field.constraints.is_unique && !field.constraints.is_primary_key {
                gorm_tags.push("unique".to_string());
            }
            if field.constraints.is_indexed && !field.constraints.is_primary_key && !field.constraints.is_unique {
                gorm_tags.push("index".to_string());
            }
            // Add gorm:type for native enums
            if let DataType::Enum { type_name: Some(_), .. } = &field.data_type {
                gorm_tags.push("type:varchar(255)".to_string()); // Will be overridden by actual enum type in DB
            }

            let exported_name = capitalize_first(&field.name);
            output.push_str(&format!(
                "\t{} {} `gorm:\"{}\" json:\"{}\"`\n",
                exported_name,
                go_type,
                gorm_tags.join(";"),
                field.name
            ));
        }

        output.push_str("}\n");
        output
    }

    fn generate_relations(&self, entity: &Entity, relations: &[Relation]) -> String {
        let mut output = String::new();
        let entity_relations: Vec<&Relation> = relations
            .iter()
            .filter(|r| r.source_entity_id == entity.id || r.target_entity_id == entity.id)
            .collect();

        for rel in entity_relations {
            let is_source = rel.source_entity_id == entity.id;
            let (target_name, _target_field) = if is_source {
                (rel.target_entity_id.clone(), rel.target_field_name.clone())
            } else {
                (rel.source_entity_id.clone(), rel.source_field_name.clone())
            };

            // Find target entity name (we need to look it up from the graph)
            let target_struct_name = capitalize_first(&target_name);

            match rel.relation_type {
                RelationType::OneToMany => {
                    if is_source {
                        // Source has many targets
                        output.push_str(&format!(
                            "\t{} []{} `gorm:\"foreignKey:{};references:{}\"`\n",
                            target_struct_name,
                            target_struct_name,
                            rel.target_field_name,
                            rel.source_field_name
                        ));
                    } else {
                        // Target belongs to source
                        let fk_field = entity.fields.iter().find(|f| f.name == rel.source_field_name);
                        let go_type = fk_field.map(|f| self.map_data_type(&f.data_type, f.constraints.is_nullable))
                            .unwrap_or_else(|| "string".to_string());
                        output.push_str(&format!(
                            "\t{} {} `gorm:\"foreignKey:{};references:{}\"`\n",
                            target_struct_name,
                            go_type,
                            rel.source_field_name,
                            rel.target_field_name
                        ));
                    }
                }
                RelationType::OneToOne => {
                    if is_source {
                        output.push_str(&format!(
                            "\t{} {} `gorm:\"foreignKey:{};references:{}\"`\n",
                            target_struct_name,
                            target_struct_name,
                            rel.target_field_name,
                            rel.source_field_name
                        ));
                    } else {
                        let fk_field = entity.fields.iter().find(|f| f.name == rel.source_field_name);
                        let go_type = fk_field.map(|f| self.map_data_type(&f.data_type, f.constraints.is_nullable))
                            .unwrap_or_else(|| "string".to_string());
                        output.push_str(&format!(
                            "\t{} {} `gorm:\"foreignKey:{};references:{}\"`\n",
                            target_struct_name,
                            go_type,
                            rel.source_field_name,
                            rel.target_field_name
                        ));
                    }
                }
                RelationType::ManyToMany => {
                    // For ManyToMany, we use the junction table generated by the compiler
                    // The junction table name is alphabetical: e.g., user_group for User↔Group
                    let source_name = if is_source {
                        entity.name.to_lowercase()
                    } else {
                        target_name.to_lowercase()
                    };
                    let other_name = if is_source {
                        target_name.to_lowercase()
                    } else {
                        entity.name.to_lowercase()
                    };
                    
                    // Determine junction table name (alphabetical)
                    let junction_name = if source_name < other_name {
                        format!("{}_{}", source_name, other_name)
                    } else {
                        format!("{}_{}", other_name, source_name)
                    };
                    
                    let source_fk = format!("{}_id", source_name);
                    let other_fk = format!("{}_id", other_name);
                    
                    // For the entity that "owns" the relation, use many2many
                    if is_source {
                        output.push_str(&format!(
                            "\t{} []{} `gorm:\"many2many:{};joinForeignKey:{};references:{}\"`\n",
                            target_struct_name,
                            target_struct_name,
                            junction_name,
                            source_fk,
                            rel.source_field_name
                        ));
                    } else {
                        output.push_str(&format!(
                            "\t{} []{} `gorm:\"many2many:{};joinForeignKey:{};references:{}\"`\n",
                            target_struct_name,
                            target_struct_name,
                            junction_name,
                            other_fk,
                            rel.target_field_name
                        ));
                    }
                }
            }
        }
        output
    }
}

fn capitalize_first(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        None => String::new(),
        Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
    }
}

pub struct PythonSqlModelDriver;

impl LanguageDriver for PythonSqlModelDriver {
    fn map_data_type(&self, data_type: &DataType, is_nullable: bool) -> String {
        let base_type = match data_type {
            DataType::String { .. } | DataType::Text => "str",
            DataType::Integer(crate::ir::IntSize::Small) => "int",
            DataType::Integer(crate::ir::IntSize::Standard) => "int",
            DataType::Integer(crate::ir::IntSize::Big) => "int",
            DataType::Float => "float",
            DataType::Decimal { .. } => "Decimal",
            DataType::Boolean => "bool",
            DataType::DateTime => "datetime",
            DataType::Json => "dict",
            DataType::Uuid => "UUID",
            DataType::Enum { values: _, type_name } => {
                // Use native PostgreSQL enum type name if available
                type_name.as_deref().unwrap_or("str")
            }
        };

        if is_nullable {
            format!("Optional[{}]", base_type)
        } else {
            base_type.to_string()
        }
    }

    fn file_extension(&self) -> &'static str {
        "py"
    }

    fn generate_model(&self, entity: &Entity) -> String {
        let mut output = String::new();

        output.push_str("from typing import Optional\nfrom datetime import datetime\nfrom decimal import Decimal\nfrom enum import Enum\nfrom sqlmodel import SQLModel, Field\n\n");

        let enum_fields: Vec<&Field> = entity
            .fields
            .iter()
            .filter(|f| matches!(f.data_type, DataType::Enum { values: _, type_name: _ }))
            .collect();

        for field in &enum_fields {
            if let DataType::Enum { values, type_name } = &field.data_type {
                if let Some(enum_type) = type_name {
                    // Native PostgreSQL enum - use the type directly
                    output.push_str(&format!("{} = str\n", enum_type));
                } else {
                    let enum_name = format!("{}Enum", capitalize_first(&field.name));
                    output.push_str(&format!("class {}(str, Enum):\n", enum_name));
                    for val in values {
                        let const_name = val.to_uppercase();
                        output.push_str(&format!("    {} = \"{}\"\n", const_name, val));
                    }
                    output.push('\n');
                }
            }
        }

        output.push_str(&format!("class {}(SQLModel, table=True):\n", entity.name));

        let pk_fields: Vec<&Field> = entity
            .fields
            .iter()
            .filter(|f| f.constraints.is_primary_key)
            .collect();
        let _has_composite_pk = pk_fields.len() > 1;

        for field in &entity.fields {
            let py_type = if let DataType::Enum { values: _, type_name } = &field.data_type {
                if let Some(enum_type) = type_name {
                    // Native PostgreSQL enum - use the type directly
                    if field.constraints.is_nullable {
                        format!("Optional[{}]", enum_type)
                    } else {
                        enum_type.to_string()
                    }
                } else {
                    let enum_name = format!("{}Enum", capitalize_first(&field.name));
                    if field.constraints.is_nullable {
                        format!("Optional[{}]", enum_name)
                    } else {
                        enum_name
                    }
                }
            } else {
                self.map_data_type(&field.data_type, field.constraints.is_nullable)
            };

            let primary_key_flag = if field.constraints.is_primary_key {
                "primary_key=True"
            } else {
                ""
            };
            
            let index_flag = if field.constraints.is_indexed && !field.constraints.is_primary_key {
                "index=True"
            } else {
                ""
            };

            let field_options = [primary_key_flag, index_flag]
                .into_iter()
                .filter(|s| !s.is_empty())
                .collect::<Vec<_>>()
                .join(", ");
            
            if field_options.is_empty() {
                output.push_str(&format!("    {}: {}\n", field.name, py_type));
            } else {
                output.push_str(&format!(
                    "    {}: {} = Field(default=None, {})\n",
                    field.name, py_type, field_options
                ));
            }
        }

        output
    }

    fn generate_relations(&self, entity: &Entity, relations: &[Relation]) -> String {
        let mut output = String::new();
        let entity_relations: Vec<&Relation> = relations
            .iter()
            .filter(|r| r.source_entity_id == entity.id || r.target_entity_id == entity.id)
            .collect();

        for rel in entity_relations {
            let is_source = rel.source_entity_id == entity.id;
            let (target_name, _target_field) = if is_source {
                (rel.target_entity_id.clone(), rel.target_field_name.clone())
            } else {
                (rel.source_entity_id.clone(), rel.source_field_name.clone())
            };

            let target_struct_name = capitalize_first(&target_name);
            let _fk_field_name = if is_source { rel.target_field_name.clone() } else { rel.source_field_name.clone() };
            let ref_field_name = if is_source { rel.source_field_name.clone() } else { rel.target_field_name.clone() };

            match rel.relation_type {
                RelationType::OneToMany => {
                    if is_source {
                        output.push_str(&format!(
                            "    {}: list[\"{}\"] = Relationship(back_populates=\"{}\")\n",
                            target_name.to_lowercase() + "s",
                            target_struct_name,
                            target_name.to_lowercase()
                        ));
                    } else {
                        output.push_str(&format!(
                            "    {}: Optional[{}] = Field(default=None, foreign_key=\"{}.{}\")\n",
                            target_name.to_lowercase(),
                            target_struct_name,
                            target_name.to_lowercase(),
                            ref_field_name
                        ));
                        output.push_str(&format!(
                            "    {}: Optional[{}] = Relationship(back_populates=\"{}s\")\n",
                            target_name.to_lowercase(),
                            target_struct_name,
                            target_name.to_lowercase()
                        ));
                    }
                }
                RelationType::OneToOne => {
                    if is_source {
                        output.push_str(&format!(
                            "    {}: Optional[{}] = Relationship(back_populates=\"{}\", sa_relationship_kwargs={{\"uselist\": False}})\n",
                            target_name.to_lowercase(),
                            target_struct_name,
                            target_name.to_lowercase()
                        ));
                    } else {
                        output.push_str(&format!(
                            "    {}: Optional[{}] = Field(default=None, foreign_key=\"{}.{}\")\n",
                            target_name.to_lowercase(),
                            target_struct_name,
                            target_name.to_lowercase(),
                            ref_field_name
                        ));
                        output.push_str(&format!(
                            "    {}: Optional[{}] = Relationship(back_populates=\"{}\", sa_relationship_kwargs={{\"uselist\": False}})\n",
                            target_name.to_lowercase(),
                            target_struct_name,
                            target_name.to_lowercase()
                        ));
                    }
                }
                RelationType::ManyToMany => {
                    // For ManyToMany, use the junction table generated by the compiler
                    let source_name = if is_source {
                        entity.name.to_lowercase()
                    } else {
                        target_name.to_lowercase()
                    };
                    let other_name = if is_source {
                        target_name.to_lowercase()
                    } else {
                        entity.name.to_lowercase()
                    };
                    
                    // Determine junction table name (alphabetical)
                    let junction_name = if source_name < other_name {
                        format!("{}_{}", source_name, other_name)
                    } else {
                        format!("{}_{}", other_name, source_name)
                    };
                    
                    let source_fk = format!("{}_id", source_name);
                    let other_fk = format!("{}_id", other_name);
                    
                    // For the entity that "owns" the relation, use link_model
                    if is_source {
                        output.push_str(&format!(
                            "    {}: list[\"{}\"] = Relationship(back_populates=\"{}s\", link_model=\"{}\")\n",
                            target_name.to_lowercase() + "s",
                            target_struct_name,
                            target_name.to_lowercase(),
                            junction_name
                        ));
                    } else {
                        output.push_str(&format!(
                            "    {}: list[\"{}\"] = Relationship(back_populates=\"{}s\", link_model=\"{}\")\n",
                            target_name.to_lowercase() + "s",
                            target_struct_name,
                            target_name.to_lowercase(),
                            junction_name
                        ));
                    }
                }
            }
        }
        output
    }
}


pub struct GoEntDriver;

impl LanguageDriver for GoEntDriver {
    fn map_data_type(&self, data_type: &DataType, is_nullable: bool) -> String {
        let base_type = match data_type {
            DataType::String { .. } | DataType::Text => "string",
            DataType::Integer(crate::ir::IntSize::Small) => "int16",
            DataType::Integer(crate::ir::IntSize::Standard) => "int",
            DataType::Integer(crate::ir::IntSize::Big) => "int64",
            DataType::Float => "float64",
            DataType::Decimal { .. } => "decimal.Decimal", // Use shopspring/decimal
            DataType::Boolean => "bool",
            DataType::DateTime => "time.Time",
            DataType::Json => "json.RawMessage",
            DataType::Uuid => "uuid.UUID",
            DataType::Enum { values: _, type_name } => {
                // Use native PostgreSQL enum type name if available
                type_name.as_deref().unwrap_or("string")
            }
        };

        if is_nullable {
            format!("*{}", base_type)
        } else {
            base_type.to_string()
        }
    }

    fn file_extension(&self) -> &'static str {
        "go"
    }

    fn generate_model(&self, entity: &Entity) -> String {
        let mut output = String::new();
        output.push_str("package models\n\n");

        let mut imports = vec![
            "\"entgo.io/ent\"",
            "\"entgo.io/ent/schema/field\"",
            "\"github.com/shopspring/decimal\"",
        ];

        let has_uuid = entity.fields.iter().any(|f| matches!(f.data_type, DataType::Uuid));
        if has_uuid {
            imports.push("\"github.com/google/uuid\"");
        }

        output.push_str(&format!("import (\n\t{}\n)\n\n", imports.join("\n\t")));

        let enum_fields: Vec<&Field> = entity
            .fields
            .iter()
            .filter(|f| matches!(f.data_type, DataType::Enum { values: _, type_name: _ }))
            .collect();

        if !enum_fields.is_empty() {
            for field in &enum_fields {
                if let DataType::Enum { values, type_name } = &field.data_type {
                    if let Some(enum_type) = type_name {
                        output.push_str(&format!("type {} = string\n\n", enum_type));
                    } else {
                        let const_name = format!("{}Status", capitalize_first(&field.name));
                        output.push_str(&format!("type {} string\n\n", const_name));
                        output.push_str("const (\n");
                        for val in values {
                            let const_val = format!("{}{}", const_name, capitalize_first(val));
                            output.push_str(&format!("\t{} {} = \"{}\"\n", const_val, const_name, val));
                        }
                        output.push_str(")\n\n");
                    }
                }
            }
        }

        output.push_str(&format!("type {} struct {{\n", entity.name));
        output.push_str("\tent.Schema\n");
        output.push_str("}\n\n");

        let pk_fields: Vec<&Field> = entity
            .fields
            .iter()
            .filter(|f| f.constraints.is_primary_key)
            .collect();
        let has_composite_pk = pk_fields.len() > 1;

        output.push_str(&format!("func ({}) Fields() []ent.Field {{\n", entity.name));
        output.push_str("\treturn []ent.Field{\n");

        for field in &entity.fields {
            let _field_type = self.map_data_type(&field.data_type, field.constraints.is_nullable);
            let _field_name = capitalize_first(&field.name);

            let ent_field_type = match field.data_type {
                DataType::Enum { values: _, type_name: _ } => "String",
                DataType::String { .. } | DataType::Text => "String",
                DataType::Integer(_) => "Int",
                DataType::Float => "Float64",
                DataType::Decimal { .. } => "Other", // Custom type for decimal
                DataType::Boolean => "Bool",
                DataType::DateTime => "Time",
                DataType::Json => "JSON",
                DataType::Uuid => "UUID",
            };

            output.push_str(&format!(
                "\t\tfield.{}(\"{}\").\n",
                ent_field_type,
                field.name
            ));

            if field.constraints.is_nullable {
                output.push_str("\t\t\tNillable().\n");
            }
            if field.constraints.is_unique && !field.constraints.is_primary_key {
                output.push_str("\t\t\tUnique().\n");
            }
            if field.constraints.is_indexed && !field.constraints.is_primary_key && !field.constraints.is_unique {
                output.push_str("\t\t\tIndex().\n");
            }
            if field.constraints.is_primary_key {
                if has_composite_pk {
                    output.push_str("\t\t\tImmutable().\n");
                } else {
                    output.push_str("\t\t\tDefault(uuid.New).\n");
                }
            }
            if matches!(field.data_type, DataType::Decimal { .. }) {
                output.push_str("\t\t\tSchemaType(map[string]string{\n");
                output.push_str("\t\t\t\tdialect.Postgres: \"numeric\",\n");
                output.push_str("\t\t\t\tdialect.MySQL: \"decimal\",\n");
                output.push_str("\t\t\t\tdialect.SQLite: \"numeric\",\n");
                output.push_str("\t\t\t}).\n");
            }
            output.push_str(&format!(
                "\t\t\tStorageKey(\"{}\"),\n",
                &field.name
            ));
        }

        output.push_str("\t}\n");
        output.push_str("}\n");
        output
    }

    fn generate_relations(&self, entity: &Entity, relations: &[Relation]) -> String {
        let mut output = String::new();
        let entity_relations: Vec<&Relation> = relations
            .iter()
            .filter(|r| r.source_entity_id == entity.id || r.target_entity_id == entity.id)
            .collect();

        for rel in entity_relations {
            let is_source = rel.source_entity_id == entity.id;
            let target_name = if is_source { rel.target_entity_id.clone() } else { rel.source_entity_id.clone() };
            let target_field = if is_source { rel.target_field_name.clone() } else { rel.source_field_name.clone() };
            let ref_field = if is_source { rel.source_field_name.clone() } else { rel.target_field_name.clone() };

            let _target_struct_name = capitalize_first(&target_name);

            match rel.relation_type {
                RelationType::OneToMany => {
                    if is_source {
                        output.push_str(&format!(
                            "\tfield.ToMany(\"{}\", \"{}\")\n",
                            target_name.to_lowercase() + "s",
                            target_name.to_lowercase()
                        ));
                    } else {
                        output.push_str(&format!(
                            "\tfield.BelongsTo(\"{}\", field.WithFK(\"{}\"))\n",
                            target_name.to_lowercase(),
                            ref_field
                        ));
                    }
                }
                RelationType::OneToOne => {
                    if is_source {
                        output.push_str(&format!(
                            "\tfield.ToOne(\"{}\", field.WithFK(\"{}\"))\n",
                            target_name.to_lowercase(),
                            target_field
                        ));
                    } else {
                        output.push_str(&format!(
                            "\tfield.BelongsTo(\"{}\", field.WithFK(\"{}\"), field.Unique())\n",
                            target_name.to_lowercase(),
                            ref_field
                        ));
                    }
                }
                RelationType::ManyToMany => {
                    // For ManyToMany, use the junction table generated by the compiler
                    let source_name = if is_source {
                        entity.name.to_lowercase()
                    } else {
                        target_name.to_lowercase()
                    };
                    let other_name = if is_source {
                        target_name.to_lowercase()
                    } else {
                        entity.name.to_lowercase()
                    };
                    
                    // Determine junction table name (alphabetical)
                    let junction_name = if source_name < other_name {
                        format!("{}_{}", source_name, other_name)
                    } else {
                        format!("{}_{}", other_name, source_name)
                    };
                    
                    let source_fk = format!("{}_id", source_name);
                    let other_fk = format!("{}_id", other_name);
                    
                    // GoEnt uses .Through() for many-to-many
                    if is_source {
                        output.push_str(&format!(
                            "\tfield.ToMany(\"{}\", \"{}\").Through(\"{}\", \"{}\", \"{}\")\n",
                            target_name.to_lowercase() + "s",
                            target_name.to_lowercase(),
                            junction_name,
                            source_fk,
                            other_fk
                        ));
                    } else {
                        output.push_str(&format!(
                            "\tfield.ToMany(\"{}\", \"{}\").Through(\"{}\", \"{}\", \"{}\")\n",
                            target_name.to_lowercase() + "s",
                            target_name.to_lowercase(),
                            junction_name,
                            other_fk,
                            source_fk
                        ));
                    }
                }
            }
        }
        output
    }
}
pub struct PythonSqlAlchemyDriver;

impl LanguageDriver for PythonSqlAlchemyDriver {
    fn map_data_type(&self, data_type: &DataType, is_nullable: bool) -> String {
        let base_type = match data_type {
            DataType::String { max_length: Some(len) } => format!("String({})", len),
            DataType::String { .. } | DataType::Text => "String".to_string(),
            DataType::Integer(crate::ir::IntSize::Small) => "SmallInteger".to_string(),
            DataType::Integer(crate::ir::IntSize::Standard) => "Integer".to_string(),
            DataType::Integer(crate::ir::IntSize::Big) => "BigInteger".to_string(),
            DataType::Float => "Float".to_string(),
            DataType::Decimal { precision, scale } => format!("Numeric(precision={}, scale={})", precision, scale),
            DataType::Boolean => "Boolean".to_string(),
            DataType::DateTime => "DateTime".to_string(),
            DataType::Json => "JSON".to_string(),
            DataType::Uuid => "Uuid".to_string(),
            DataType::Enum { values, type_name } => {
                if let Some(enum_type) = type_name {
                    // Native PostgreSQL enum - use the type name
                    enum_type.to_string()
                } else {
                    let enum_vals = values.iter().map(|v| format!("'{}'", v)).collect::<Vec<_>>().join(", ");
                    format!("Enum({})", enum_vals)
                }
            },
        };

        if is_nullable {
            format!("{}(nullable=True)", base_type)
        } else {
            format!("{}(nullable=False)", base_type)
        }
    }

    fn file_extension(&self) -> &'static str {
        "py"
    }

    fn generate_model(&self, entity: &Entity) -> String {
        let mut output = String::new();

        output.push_str("from sqlalchemy import Column, Integer, String, Boolean, DateTime, Float, Text, JSON, Enum\n");
        output.push_str("from sqlalchemy.orm import declarative_base\n");
        output.push_str("from sqlalchemy.dialects.postgresql import UUID\n\n");

        output.push_str("Base = declarative_base()\n\n");

        output.push_str(&format!("class {}(Base):\n", entity.name));
        output.push_str(&format!("    __tablename__ = '{}'\n\n", entity.name.to_lowercase()));

        let pk_fields: Vec<&Field> = entity
            .fields
            .iter()
            .filter(|f| f.constraints.is_primary_key)
            .collect();
        let _has_composite_pk = pk_fields.len() > 1;

        for field in &entity.fields {
            let col_type = self.map_data_type(&field.data_type, field.constraints.is_nullable);

            let mut constraints = String::new();
            if field.constraints.is_primary_key {
                constraints.push_str(", primary_key=True");
            }
            if field.constraints.is_unique && !field.constraints.is_primary_key {
                constraints.push_str(", unique=True");
            }
            if field.constraints.is_indexed && !field.constraints.is_primary_key && !field.constraints.is_unique {
                constraints.push_str(", index=True");
            }

            output.push_str(&format!(
                "    {} = Column({}{}, default=None)\n",
                field.name, col_type, constraints
            ));
        }

        output
    }

    fn generate_relations(&self, entity: &Entity, relations: &[Relation]) -> String {
        let mut output = String::new();
        let entity_relations: Vec<&Relation> = relations
            .iter()
            .filter(|r| r.source_entity_id == entity.id || r.target_entity_id == entity.id)
            .collect();

        for rel in entity_relations {
            let is_source = rel.source_entity_id == entity.id;
            let target_name = if is_source { rel.target_entity_id.clone() } else { rel.source_entity_id.clone() };
            let ref_field = if is_source { rel.source_field_name.clone() } else { rel.target_field_name.clone() };
            let target_field = if is_source { rel.target_field_name.clone() } else { rel.source_field_name.clone() };

            let target_struct_name = target_name.clone();

            let on_delete = rel.on_delete.as_ref().map(|a| self.map_referential_action(a)).unwrap_or_else(|| "CASCADE".to_string());
            let on_update = rel.on_update.as_ref().map(|a| self.map_referential_action(a)).unwrap_or_else(|| "CASCADE".to_string());

            match rel.relation_type {
                RelationType::OneToMany => {
                    if is_source {
                        output.push_str(&format!(
                            "    {} = relationship(\"{}\", back_populates=\"{}\")\n",
                            target_struct_name.to_lowercase() + "s",
                            target_struct_name,
                            entity.name.to_lowercase()
                        ));
                    } else {
                        output.push_str(&format!(
                            "    {} = Column(ForeignKey('{}.{}', ondelete='{}', onupdate='{}'))\n",
                            ref_field,
                            target_struct_name.to_lowercase(),
                            target_field,
                            on_delete,
                            on_update
                        ));
                        output.push_str(&format!(
                            "    {} = relationship(\"{}\", back_populates=\"{}s\")\n",
                            target_struct_name.to_lowercase(),
                            target_struct_name,
                            entity.name.to_lowercase()
                        ));
                    }
                }
                RelationType::OneToOne => {
                    if is_source {
                        output.push_str(&format!(
                            "    {} = relationship(\"{}\", back_populates=\"{}\", uselist=False)\n",
                            target_struct_name.to_lowercase(),
                            target_struct_name,
                            entity.name.to_lowercase()
                        ));
                    } else {
                        output.push_str(&format!(
                            "    {} = Column(ForeignKey('{}.{}', ondelete='{}', onupdate='{}'), unique=True)\n",
                            ref_field,
                            target_struct_name.to_lowercase(),
                            target_field,
                            on_delete,
                            on_update
                        ));
                        output.push_str(&format!(
                            "    {} = relationship(\"{}\", back_populates=\"{}\", uselist=False)\n",
                            target_struct_name.to_lowercase(),
                            target_struct_name,
                            entity.name.to_lowercase()
                        ));
                    }
                }
                RelationType::ManyToMany => {
                    // For ManyToMany, use the junction table generated by the compiler
                    let source_name = if is_source {
                        entity.name.to_lowercase()
                    } else {
                        target_name.to_lowercase()
                    };
                    let other_name = if is_source {
                        target_name.to_lowercase()
                    } else {
                        entity.name.to_lowercase()
                    };
                    
                    // Determine junction table name (alphabetical)
                    let junction_name = if source_name < other_name {
                        format!("{}_{}", source_name, other_name)
                    } else {
                        format!("{}_{}", other_name, source_name)
                    };
                    
                    // SQLAlchemy uses secondary argument for many-to-many
                    if is_source {
                        output.push_str(&format!(
                            "    {} = relationship(\"{}\", secondary=\"{}\", back_populates=\"{}s\")\n",
                            target_name.to_lowercase() + "s",
                            target_struct_name,
                            junction_name,
                            target_name.to_lowercase()
                        ));
                    } else {
                        output.push_str(&format!(
                            "    {} = relationship(\"{}\", secondary=\"{}\", back_populates=\"{}s\")\n",
                            target_name.to_lowercase() + "s",
                            target_struct_name,
                            junction_name,
                            target_name.to_lowercase()
                        ));
                    }
                }
            }
        }
        output
    }
}
pub struct RustDieselDriver;

impl LanguageDriver for RustDieselDriver {
    fn map_data_type(&self, data_type: &DataType, _is_nullable: bool) -> String {
        match data_type {
            DataType::String { .. } | DataType::Text => "String".to_string(),
            DataType::Integer(crate::ir::IntSize::Small) => "i16".to_string(),
            DataType::Integer(crate::ir::IntSize::Standard) => "i32".to_string(),
            DataType::Integer(crate::ir::IntSize::Big) => "i64".to_string(),
            DataType::Float => "f64".to_string(),
            DataType::Decimal { .. } => "BigDecimal".to_string(),
            DataType::Boolean => "bool".to_string(),
            DataType::DateTime => "chrono::NaiveDateTime".to_string(),
            DataType::Json => "serde_json::Value".to_string(),
            DataType::Uuid => "uuid::Uuid".to_string(),
            DataType::Enum { values: _, type_name } => {
                type_name.as_deref().unwrap_or("String").to_string()
            }
        }
    }

    fn file_extension(&self) -> &'static str {
        "rs"
    }

    fn generate_model(&self, entity: &Entity) -> String {
        let mut output = String::new();

        output.push_str("use diesel::prelude::*;\nuse serde::{Deserialize, Serialize};\nuse diesel::sql_types::{Text, Integer, BigInt, Float, Double, Boolean, Timestamp, Jsonb, Uuid, Numeric};\nuse bigdecimal::BigDecimal;\n\n");

        let enum_fields: Vec<&Field> = entity
            .fields
            .iter()
            .filter(|f| matches!(f.data_type, DataType::Enum { values: _, type_name: _ }))
            .collect();

        for field in &enum_fields {
            if let DataType::Enum { values, type_name } = &field.data_type {
                if let Some(enum_type) = type_name {
                    // Native PostgreSQL enum - emit type alias
                    output.push_str(&format!("type {} = String;\n\n", enum_type));
                } else {
                    let enum_name = format!("{}Enum", capitalize_first(&field.name));
                    output.push_str("#[derive(Debug, Clone, Copy, PartialEq, Eq, diesel::deserialize::FromSqlRow, diesel::serialize::ToSql)]\n");
                    output.push_str("#[diesel(sql_type = Text)]\n");
                    output.push_str(&format!("pub enum {} {{\n", enum_name));
                    for val in values {
                        let variant = capitalize_first(val);
                        output.push_str(&format!("    {},\n", variant));
                    }
                    output.push_str("}\n\n");

                    output.push_str(&format!("impl diesel::serialize::ToSql<Text, diesel::pg::Pg> for {} {{\n", enum_name));
                    output.push_str("    fn to_sql<'b>(&'b self, out: &mut diesel::serialize::Output<'b, '_, diesel::pg::Pg>) -> diesel::serialize::Result {\n");
                    output.push_str("        let s = match self {\n");
                    for val in values {
                        let variant = capitalize_first(val);
                        output.push_str(&format!("            {}::{} => \"{}\",\n", enum_name, variant, val));
                    }
                    output.push_str("        };\n");
                    output.push_str("        out.write_all(s.as_bytes())?;\n");
                    output.push_str("        Ok(diesel::serialize::IsNull::No)\n");
                    output.push_str("    }\n");
                    output.push_str("}\n\n");

                    output.push_str(&format!("impl diesel::deserialize::FromSql<Text, diesel::pg::Pg> for {} {{\n", enum_name));
                    output.push_str("    fn from_sql(bytes: diesel::backend::RawValue<'_, diesel::pg::Pg>) -> diesel::deserialize::Result<Self> {\n");
                    output.push_str("        let s = std::str::from_utf8(bytes.as_bytes())?;\n");
                    output.push_str("        match s {\n");
                    for val in values {
                        let variant = capitalize_first(val);
                        output.push_str(&format!("            \"{}\" => Ok({}::{}),\n", val, enum_name, variant));
                    }
                    output.push_str("            _ => Err(format!(\"Invalid variant for {}: {}\", stringify!({}), s).into()),\n");
                    output.push_str("        }\n");
                    output.push_str("    }\n");
                    output.push_str("}\n\n");
                }
            }
        }

        output.push_str("#[derive(Queryable, Insertable, Selectable, Serialize, Deserialize, Debug, Clone)]\n");
        output.push_str(&format!("#[diesel(table_name = {})]\n", entity.name.to_lowercase()));
        output.push_str(&format!("pub struct {} {{\n", entity.name));

        for field in &entity.fields {
            let rust_type = match &field.data_type {
                DataType::Enum { values: _, type_name } => {
                    type_name.as_deref().unwrap_or(&format!("{}Enum", capitalize_first(&field.name))).to_string()
                }
                _ => self.map_data_type(&field.data_type, field.constraints.is_nullable),
            };

            let final_type = if field.constraints.is_nullable {
                format!("Option<{}>", rust_type)
            } else {
                rust_type
            };

            if field.constraints.is_indexed && !field.constraints.is_primary_key {
                output.push_str("#[diesel(index)]\n");
            }
            output.push_str(&format!("    pub {}: {},\n", field.name, final_type));
        }

        output.push_str("}\n");
        output
    }

    fn generate_relations(&self, entity: &Entity, relations: &[Relation]) -> String {
        let mut output = String::new();
        let entity_relations: Vec<&Relation> = relations
            .iter()
            .filter(|r| r.source_entity_id == entity.id || r.target_entity_id == entity.id)
            .collect();

        for rel in entity_relations {
            let is_source = rel.source_entity_id == entity.id;
            let target_name = if is_source { rel.target_entity_id.clone() } else { rel.source_entity_id.clone() };
            let ref_field = if is_source { rel.source_field_name.clone() } else { rel.target_field_name.clone() };
            let target_field = if is_source { rel.target_field_name.clone() } else { rel.source_field_name.clone() };

            let _target_struct_name = capitalize_first(&target_name);

            match rel.relation_type {
                RelationType::OneToMany => {
                    if is_source {
                        output.push_str(&format!(
                            "    // Has many {}: use Queryable with filter on {}.{}\n",
                            target_name.to_lowercase(),
                            target_name.to_lowercase(),
                            ref_field
                        ));
                    } else {
                        output.push_str(&format!(
                            "    // Belongs to {}: {} is FK to {}.{}\n",
                            target_name.to_lowercase(),
                            ref_field,
                            target_name.to_lowercase(),
                            target_field
                        ));
                    }
                }
                RelationType::OneToOne => {
                    if is_source {
                        output.push_str(&format!(
                            "    // Has one {}: {} is FK to {}.{}\n",
                            target_name.to_lowercase(),
                            target_field,
                            target_name.to_lowercase(),
                            target_field
                        ));
                    } else {
                        output.push_str(&format!(
                            "    // Belongs to {}: {} is FK to {}.{} (unique)\n",
                            target_name.to_lowercase(),
                            ref_field,
                            target_name.to_lowercase(),
                            target_field
                        ));
                    }
                }
                RelationType::ManyToMany => {
                    // For ManyToMany, use the junction table generated by the compiler
                    let source_name = if is_source {
                        entity.name.to_lowercase()
                    } else {
                        target_name.to_lowercase()
                    };
                    let other_name = if is_source {
                        target_name.to_lowercase()
                    } else {
                        entity.name.to_lowercase()
                    };
                    
                    // Determine junction table name (alphabetical)
                    let junction_name = if source_name < other_name {
                        format!("{}_{}", source_name, other_name)
                    } else {
                        format!("{}_{}", other_name, source_name)
                    };
                    
                    // Diesel uses joinable! macro for many-to-many
                    if is_source {
                        output.push_str(&format!(
                            "    // Has many {}: via junction table {}\n",
                            target_name.to_lowercase(),
                            junction_name
                        ));
                        output.push_str(&format!(
                            "    // joinable!({} -> {} ({}))\n",
                            entity.name.to_lowercase(),
                            junction_name,
                            target_name.to_lowercase()
                        ));
                    } else {
                        output.push_str(&format!(
                            "    // Has many {}: via junction table {}\n",
                            target_name.to_lowercase(),
                            junction_name
                        ));
                        output.push_str(&format!(
                            "    // joinable!({} -> {} ({}))\n",
                            entity.name.to_lowercase(),
                            junction_name,
                            target_name.to_lowercase()
                        ));
                    }
                }
            }
        }
        output
    }
}
pub struct RustSeaOrmDriver;

impl LanguageDriver for RustSeaOrmDriver {
    fn map_data_type(&self, data_type: &DataType, is_nullable: bool) -> String {
        let base_type = match data_type {
            DataType::String { .. } | DataType::Text => "String".to_string(),
            DataType::Integer(crate::ir::IntSize::Small) => "i16".to_string(),
            DataType::Integer(crate::ir::IntSize::Standard) => "i32".to_string(),
            DataType::Integer(crate::ir::IntSize::Big) => "i64".to_string(),
            DataType::Float => "f64".to_string(),
            DataType::Decimal { .. } => "Decimal".to_string(),
            DataType::Boolean => "bool".to_string(),
            DataType::DateTime => "DateTime".to_string(),
            DataType::Json => "Json".to_string(),
            DataType::Uuid => "Uuid".to_string(),
            DataType::Enum { values: _, type_name } => {
                type_name.as_deref().unwrap_or("String").to_string()
            }
        };

        if is_nullable {
            format!("Option<{}>", base_type)
        } else {
            base_type
        }
    }

    fn file_extension(&self) -> &'static str {
        "rs"
    }

    fn generate_model(&self, entity: &Entity) -> String {
        let mut output = String::new();

        output.push_str("use sea_orm::entity::prelude::*;\nuse serde::{Deserialize, Serialize};\nuse sea_orm::EnumIter;\nuse std::fmt;\n\n");

        let enum_fields: Vec<&Field> = entity
            .fields
            .iter()
            .filter(|f| matches!(f.data_type, DataType::Enum { values: _, type_name: _ }))
            .collect();

        for field in &enum_fields {
            if let DataType::Enum { values, type_name } = &field.data_type {
                if let Some(enum_type) = type_name {
                    // Native PostgreSQL enum - emit type alias for the enum
                    output.push_str(&format!("type {} = String;\n\n", enum_type));
                } else {
                    let enum_name = format!("{}Enum", capitalize_first(&field.name));
                    output.push_str("#[derive(Debug, Clone, PartialEq, EnumIter, DeriveActiveEnum, Serialize, Deserialize)]\n");
                    output.push_str(&format!("#[sea_orm(rs_type = \"String\", db_type = \"Enum\", enum_name = \"{}\")]\n", field.name));
                    output.push_str(&format!("pub enum {} {{\n", enum_name));
                    for val in values {
                        let variant = capitalize_first(val);
                        output.push_str(&format!("    #[sea_orm(string_value = \"{}\")]\n", val));
                        output.push_str(&format!("    {},\n", variant));
                    }
                    output.push_str("}\n\n");
                }
            }
        }

        output.push_str("#[derive(Clone, Debug, PartialEq, DeriveModel, DeriveActiveModel, Serialize, Deserialize)]\n");
        output.push_str(&format!("#[sea_orm(table_name = \"{}\")]\n", entity.name.to_lowercase()));
        output.push_str("pub struct Model {\n");

        for field in &entity.fields {
            let sea_type = if let DataType::Enum { values: _, type_name } = &field.data_type {
                if let Some(enum_type) = type_name {
                    if field.constraints.is_nullable {
                        format!("Option<{}>", enum_type)
                    } else {
                        enum_type.to_string()
                    }
                } else {
                    let enum_name = format!("{}Enum", capitalize_first(&field.name));
                    if field.constraints.is_nullable {
                        format!("Option<{}>", enum_name)
                    } else {
                        enum_name
                    }
                }
            } else {
                self.map_data_type(&field.data_type, field.constraints.is_nullable)
            };

        let pk_fields: Vec<&Field> = entity
            .fields
            .iter()
            .filter(|f| f.constraints.is_primary_key)
            .collect();
        let _has_composite_pk = pk_fields.len() > 1;

            let mut attributes = String::new();
            if field.constraints.is_primary_key {
                attributes.push_str("primary_key");
            } else if field.constraints.is_unique && !field.constraints.is_primary_key {
                attributes.push_str("unique");
            } else if field.constraints.is_indexed && !field.constraints.is_primary_key && !field.constraints.is_unique {
                attributes.push_str("index");
            }

            if !attributes.is_empty() {
                output.push_str(&format!("    #[sea_orm({})]\n", attributes));
            }

            output.push_str(&format!("    pub {}: {},\n", field.name, sea_type));
        }

        output.push_str("}\n");
        output
    }

    fn generate_relations(&self, entity: &Entity, relations: &[Relation]) -> String {
        let mut output = String::new();
        let entity_relations: Vec<&Relation> = relations
            .iter()
            .filter(|r| r.source_entity_id == entity.id || r.target_entity_id == entity.id)
            .collect();

        for rel in entity_relations {
            let is_source = rel.source_entity_id == entity.id;
            let target_name = if is_source { rel.target_entity_id.clone() } else { rel.source_entity_id.clone() };
            let ref_field = if is_source { rel.source_field_name.clone() } else { rel.target_field_name.clone() };
            let target_field = if is_source { rel.target_field_name.clone() } else { rel.source_field_name.clone() };

            let _target_struct_name = capitalize_first(&target_name);
            let _on_delete = rel.on_delete.as_ref().map(|a| self.map_referential_action(a)).unwrap_or_else(|| "Cascade".to_string());
            let _on_update = rel.on_update.as_ref().map(|a| self.map_referential_action(a)).unwrap_or_else(|| "Cascade".to_string());

            match rel.relation_type {
                RelationType::OneToMany => {
                    if is_source {
                        output.push_str(&format!(
                            "    // Has many {}: use Related to load\n",
                            target_name.to_lowercase()
                        ));
                        output.push_str(&format!(
                            "    // relation: {}.{} -> {}.{}\n",
                            entity.name.to_lowercase(), ref_field,
                            target_name.to_lowercase(), target_field
                        ));
                    } else {
                        output.push_str(&format!(
                            "    // Belongs to {}: {} -> {}.{}\n",
                            target_name.to_lowercase(),
                            ref_field,
                            target_name.to_lowercase(), target_field
                        ));
                    }
                }
                RelationType::OneToOne => {
                    if is_source {
                        output.push_str(&format!(
                            "    // Has one {}: {} -> {}.{}\n",
                            target_name.to_lowercase(),
                            target_field,
                            target_name.to_lowercase(), target_field
                        ));
                    } else {
                        output.push_str(&format!(
                            "    // Belongs to {}: {} -> {}.{} (unique)\n",
                            target_name.to_lowercase(),
                            ref_field,
                            target_name.to_lowercase(), target_field
                        ));
                    }
                }
                RelationType::ManyToMany => {
                    // For ManyToMany, use the junction table generated by the compiler
                    let source_name = if is_source {
                        entity.name.to_lowercase()
                    } else {
                        target_name.to_lowercase()
                    };
                    let other_name = if is_source {
                        target_name.to_lowercase()
                    } else {
                        entity.name.to_lowercase()
                    };
                    
                    // Determine junction table name (alphabetical)
                    let junction_name = if source_name < other_name {
                        format!("{}_{}", source_name, other_name)
                    } else {
                        format!("{}_{}", other_name, source_name)
                    };
                    
                    // SeaORM uses Link for many-to-many
                    let junction_struct = capitalize_first(&junction_name);
                    if is_source {
                        output.push_str(&format!(
                            "    // Has many {}: via junction table {}\n",
                            target_name.to_lowercase(),
                            junction_name
                        ));
                        output.push_str(&format!(
                            "    // Related link: Entity::has_many({}).through({})\n",
                            target_name.to_lowercase(),
                            junction_struct
                        ));
                    } else {
                        output.push_str(&format!(
                            "    // Has many {}: via junction table {}\n",
                            target_name.to_lowercase(),
                            junction_name
                        ));
                        output.push_str(&format!(
                            "    // Related link: Entity::has_many({}).through({})\n",
                            target_name.to_lowercase(),
                            junction_struct
                        ));
                    }
                }
            }
        }
        output
    }
}
pub struct JavaScriptSequelizeDriver;

impl LanguageDriver for JavaScriptSequelizeDriver {
    fn map_data_type(&self, data_type: &DataType, _is_nullable: bool) -> String {
        match data_type {
            DataType::String { max_length: Some(len) } => {
                format!("DataTypes.STRING({})", len)
            }
            DataType::String { .. } | DataType::Text => "DataTypes.TEXT".to_string(),
            DataType::Integer(crate::ir::IntSize::Small) => "DataTypes.SMALLINT".to_string(),
            DataType::Integer(crate::ir::IntSize::Standard) => "DataTypes.INTEGER".to_string(),
            DataType::Integer(crate::ir::IntSize::Big) => "DataTypes.BIGINT".to_string(),
            DataType::Float => "DataTypes.FLOAT".to_string(),
            DataType::Decimal { precision, scale } => format!("DataTypes.DECIMAL({}, {})", precision, scale),
            DataType::Boolean => "DataTypes.BOOLEAN".to_string(),
            DataType::DateTime => "DataTypes.DATE".to_string(),
            DataType::Json => "DataTypes.JSON".to_string(),
            DataType::Uuid => "DataTypes.UUID".to_string(),
            DataType::Enum { values, type_name } => {
                if let Some(enum_type) = type_name {
                    // Native PostgreSQL enum - use custom type
                    format!("DataTypes.ENUM(\"{}\")", enum_type)
                } else {
                    let enum_vals = values.iter().map(|v| format!("'{}'", v)).collect::<Vec<_>>().join(", ");
                    format!("DataTypes.ENUM({})", enum_vals)
                }
            },
        }
    }

    fn file_extension(&self) -> &'static str {
        "js"
    }

    fn generate_model(&self, entity: &Entity) -> String {
        let mut output = String::new();

        output.push_str("module.exports = (sequelize, DataTypes) => {\n");
        output.push_str(&format!("  const {} = sequelize.define('{}', {{\n", entity.name, entity.name));

        let pk_fields: Vec<&Field> = entity
            .fields
            .iter()
            .filter(|f| f.constraints.is_primary_key)
            .collect();
        let _has_composite_pk = pk_fields.len() > 1;

        for field in &entity.fields {
            let base_type = self.map_data_type(&field.data_type, field.constraints.is_nullable);

            let mut field_config = format!("type: {}", base_type);
            if field.constraints.is_primary_key {
                field_config.push_str(", primaryKey: true");
            }
            if field.constraints.is_unique && !field.constraints.is_primary_key {
                field_config.push_str(", unique: true");
            }
            if field.constraints.is_indexed && !field.constraints.is_primary_key && !field.constraints.is_unique {
                field_config.push_str(", index: true");
            }
            if field.constraints.is_nullable {
                field_config.push_str(", allowNull: true");
            } else {
                field_config.push_str(", allowNull: false");
            }

            output.push_str(&format!("    {}: {{ {} }},\n", field.name, field_config));
        }

        output.push_str("  });\n");
        output.push_str(&format!("  return {};\n", entity.name));
        output.push_str("};\n");
        output
    }

    fn generate_relations(&self, entity: &Entity, relations: &[Relation]) -> String {
        let mut output = String::new();
        let entity_relations: Vec<&Relation> = relations
            .iter()
            .filter(|r| r.source_entity_id == entity.id || r.target_entity_id == entity.id)
            .collect();

        for rel in entity_relations {
            let is_source = rel.source_entity_id == entity.id;
            let target_name = if is_source { rel.target_entity_id.clone() } else { rel.source_entity_id.clone() };
            let ref_field = if is_source { rel.source_field_name.clone() } else { rel.target_field_name.clone() };
            let target_field = if is_source { rel.target_field_name.clone() } else { rel.source_field_name.clone() };

            let target_struct_name = target_name.clone();
            let on_delete = rel.on_delete.as_ref().map(|a| self.map_referential_action(a)).unwrap_or_else(|| "CASCADE".to_string());
            let on_update = rel.on_update.as_ref().map(|a| self.map_referential_action(a)).unwrap_or_else(|| "CASCADE".to_string());

            match rel.relation_type {
                RelationType::OneToMany => {
                    if is_source {
                        output.push_str(&format!(
                            "  {}.hasMany({}, {{ foreignKey: '{}', sourceKey: '{}', onDelete: '{}', onUpdate: '{}' }});\n",
                            entity.name,
                            target_struct_name,
                            target_field,
                            ref_field,
                            on_delete,
                            on_update
                        ));
                    } else {
                        output.push_str(&format!(
                            "  {}.belongsTo({}, {{ foreignKey: '{}', targetKey: '{}', onDelete: '{}', onUpdate: '{}' }});\n",
                            entity.name,
                            target_struct_name,
                            ref_field,
                            target_field,
                            on_delete,
                            on_update
                        ));
                    }
                }
                RelationType::OneToOne => {
                    if is_source {
                        output.push_str(&format!(
                            "  {}.hasOne({}, {{ foreignKey: '{}', sourceKey: '{}', onDelete: '{}', onUpdate: '{}' }});\n",
                            entity.name,
                            target_struct_name,
                            target_field,
                            ref_field,
                            on_delete,
                            on_update
                        ));
                    } else {
                        output.push_str(&format!(
                            "  {}.belongsTo({}, {{ foreignKey: '{}', targetKey: '{}', onDelete: '{}', onUpdate: '{}' }});\n",
                            entity.name,
                            target_struct_name,
                            ref_field,
                            target_field,
                            on_delete,
                            on_update
                        ));
                    }
                }
                RelationType::ManyToMany => {
                    // For ManyToMany, use the junction table generated by the compiler
                    let source_name = if is_source {
                        entity.name.to_lowercase()
                    } else {
                        target_name.to_lowercase()
                    };
                    let other_name = if is_source {
                        target_name.to_lowercase()
                    } else {
                        entity.name.to_lowercase()
                    };
                    
                    // Determine junction table name (alphabetical)
                    let junction_name = if source_name < other_name {
                        format!("{}_{}", source_name, other_name)
                    } else {
                        format!("{}_{}", other_name, source_name)
                    };
                    
                    // Sequelize uses belongsToMany for many-to-many
                    let source_fk = format!("{}_id", source_name);
                    let other_fk = format!("{}_id", other_name);
                    
                    if is_source {
                        output.push_str(&format!(
                            "  {}.belongsToMany({}, {{ through: '{}', foreignKey: '{}', otherKey: '{}', onDelete: '{}', onUpdate: '{}' }});\n",
                            entity.name,
                            target_struct_name,
                            junction_name,
                            source_fk,
                            other_fk,
                            on_delete,
                            on_update
                        ));
                    } else {
                        output.push_str(&format!(
                            "  {}.belongsToMany({}, {{ through: '{}', foreignKey: '{}', otherKey: '{}', onDelete: '{}', onUpdate: '{}' }});\n",
                            entity.name,
                            target_struct_name,
                            junction_name,
                            other_fk,
                            source_fk,
                            on_delete,
                            on_update
                        ));
                    }
                }
            }
        }
        output
    }
}
pub struct JavaScriptTypeOrmDriver;

impl LanguageDriver for JavaScriptTypeOrmDriver {
    fn map_data_type(&self, data_type: &DataType, _is_nullable: bool) -> String {
        match data_type {
            DataType::String { .. } => "varchar".to_string(),
            DataType::Text => "text".to_string(),
            DataType::Integer(crate::ir::IntSize::Small) => "smallint".to_string(),
            DataType::Integer(crate::ir::IntSize::Standard) => "int".to_string(),
            DataType::Integer(crate::ir::IntSize::Big) => "bigint".to_string(),
            DataType::Float => "float".to_string(),
            DataType::Decimal { precision, scale } => format!("decimal({}, {})", precision, scale),
            DataType::Boolean => "boolean".to_string(),
            DataType::DateTime => "timestamp".to_string(),
            DataType::Json => "json".to_string(),
            DataType::Uuid => "uuid".to_string(),
            DataType::Enum { values, type_name } => {
                if let Some(enum_type) = type_name {
                    // Native PostgreSQL enum - use custom type
                    format!("enum(\"{}\")", enum_type)
                } else {
                    let enum_vals = values.iter().map(|v| format!("'{}'", v)).collect::<Vec<_>>().join(", ");
                    format!("enum({})", enum_vals)
                }
            },
        }
    }

    fn file_extension(&self) -> &'static str {
        "ts"
    }

    fn generate_model(&self, entity: &Entity) -> String {
        let mut output = String::new();

        let pk_fields: Vec<&Field> = entity
            .fields
            .iter()
            .filter(|f| f.constraints.is_primary_key)
            .collect();
        let has_composite_pk = pk_fields.len() > 1;

        let import = if has_composite_pk {
            "import { Entity, PrimaryColumn, Column } from 'typeorm';\n\n"
        } else {
            "import { Entity, PrimaryGeneratedColumn, Column } from 'typeorm';\n\n"
        };
        output.push_str(import);

        output.push_str(&format!("@Entity('{}')\n", entity.name.to_lowercase()));
        output.push_str(&format!("export class {} {{\n", entity.name));

        for field in &entity.fields {
            if field.constraints.is_primary_key {
                if has_composite_pk {
                    output.push_str("  @PrimaryColumn()\n");
                } else {
                    output.push_str("  @PrimaryGeneratedColumn('uuid')\n");
                }
                output.push_str(&format!("  {}: string;\n\n", field.name));
            } else {
                let col_type = self.map_data_type(&field.data_type, field.constraints.is_nullable);
                let mut col_options = String::new();

                if field.constraints.is_unique {
                    col_options.push_str("unique: true, ");
                }
                if field.constraints.is_indexed && !field.constraints.is_unique {
                    col_options.push_str("index: true, ");
                }
                if field.constraints.is_nullable {
                    col_options.push_str("nullable: true, ");
                }

                output.push_str("  @Column({\n");
                output.push_str(&format!("    type: '{}',\n", col_type));
                if !col_options.is_empty() {
                    output.push_str(&format!("    {}\n", col_options));
                }
                output.push_str("  })\n");

                let ts_type = match &field.data_type {
                    DataType::Boolean => "boolean".to_string(),
                    DataType::Integer(_) => "number".to_string(),
                    DataType::Float | DataType::Decimal { .. } => "number".to_string(),
                    DataType::Enum { values: _, type_name: _ } => format!("{}Enum", capitalize_first(&field.name)),
                    _ => "string".to_string(),
                };

                output.push_str(&format!("  {}: {};\n\n", field.name, ts_type));
            }
        }

        // Add enum type definitions at the end
        let enum_fields: Vec<&Field> = entity
            .fields
            .iter()
            .filter(|f| matches!(f.data_type, DataType::Enum { values: _, type_name: _ }))
            .collect();

        if !enum_fields.is_empty() {
            for field in &enum_fields {
                if let DataType::Enum { values, type_name: _ } = &field.data_type {
                    let enum_name = format!("{}Enum", capitalize_first(&field.name));
                    output.push_str(&format!("export enum {} {{\n", enum_name));
                    for val in values {
                        let const_name = val.to_uppercase();
                        output.push_str(&format!("    {} = '{}',\n", const_name, val));
                    }
                    output.push_str("}\n\n");
                }
            }
        }

        output.push_str("}\n");
        output
    }

    fn generate_relations(&self, entity: &Entity, relations: &[Relation]) -> String {
        let mut output = String::new();
        let entity_relations: Vec<&Relation> = relations
            .iter()
            .filter(|r| r.source_entity_id == entity.id || r.target_entity_id == entity.id)
            .collect();

        for rel in entity_relations {
            let is_source = rel.source_entity_id == entity.id;
            let target_name = if is_source { rel.target_entity_id.clone() } else { rel.source_entity_id.clone() };
            let ref_field = if is_source { rel.source_field_name.clone() } else { rel.target_field_name.clone() };
            let target_field = if is_source { rel.target_field_name.clone() } else { rel.source_field_name.clone() };

            let target_struct_name = target_name.clone();
            let on_delete = rel.on_delete.as_ref().map(|a| self.map_referential_action(a)).unwrap_or_else(|| "CASCADE".to_string());
            let on_update = rel.on_update.as_ref().map(|a| self.map_referential_action(a)).unwrap_or_else(|| "CASCADE".to_string());

            match rel.relation_type {
                RelationType::OneToMany => {
                    if is_source {
                        output.push_str(&format!(
                            "  @OneToMany(() => {}, {} => {}.{})\n",
                            target_struct_name,
                            target_struct_name.to_lowercase(),
                            entity.name.to_lowercase(),
                            ref_field
                        ));
                    } else {
                        output.push_str(&format!(
                            "  @ManyToOne(() => {}, {} => {}.{})\n",
                            target_struct_name,
                            target_struct_name.to_lowercase(),
                            target_struct_name.to_lowercase(),
                            target_field
                        ));
                        output.push_str(&format!(
                            "  @JoinColumn({{ name: '{}', referencedColumnName: '{}', onDelete: '{}', onUpdate: '{}' }})\n",
                            ref_field, target_field, on_delete, on_update
                        ));
                    }
                }
                RelationType::OneToOne => {
                    if is_source {
                        output.push_str(&format!(
                            "  @OneToOne(() => {}, {{ nullable: true }})\n",
                            target_struct_name
                        ));
                        output.push_str(&format!(
                            "  @JoinColumn({{ name: '{}', referencedColumnName: '{}', onDelete: '{}', onUpdate: '{}' }})\n",
                            target_field, target_field, on_delete, on_update
                        ));
                    } else {
                        output.push_str(&format!(
                            "  @OneToOne(() => {}, {{ nullable: true }})\n",
                            target_struct_name
                        ));
                        output.push_str(&format!(
                            "  @JoinColumn({{ name: '{}', referencedColumnName: '{}', onDelete: '{}', onUpdate: '{}' }})\n",
                            ref_field, target_field, on_delete, on_update
                        ));
                    }
                }
                RelationType::ManyToMany => {
                    // For ManyToMany, Prisma uses the junction table generated by the compiler
                    let source_name = if is_source {
                        entity.name.to_lowercase()
                    } else {
                        target_name.to_lowercase()
                    };
                    let other_name = if is_source {
                        target_name.to_lowercase()
                    } else {
                        entity.name.to_lowercase()
                    };
                    
                    // Determine junction table name (alphabetical)
                    let junction_name = if source_name < other_name {
                        format!("{}_{}", source_name, other_name)
                    } else {
                        format!("{}_{}", other_name, source_name)
                    };
                    
                    // Prisma uses implicit many-to-many via the junction table model
                    // The junction table will be defined separately with two relation fields
                    if is_source {
                        output.push_str(&format!(
                            "  {} {}[] @relation(\"{}{}\")\n",
                            target_struct_name.to_lowercase(),
                            target_struct_name,
                            entity.name,
                            target_struct_name
                        ));
                    } else {
                        output.push_str(&format!(
                            "  {} {}[] @relation(\"{}{}\")\n",
                            target_struct_name.to_lowercase(),
                            target_struct_name,
                            entity.name,
                            target_struct_name
                        ));
                    }
                }
            }
        }
        output
    }
}
pub struct TypeScriptPrismaDriver;

impl LanguageDriver for TypeScriptPrismaDriver {
    fn map_data_type(&self, data_type: &DataType, _is_nullable: bool) -> String {
        match data_type {
            DataType::String { .. } => "String".to_string(),
            DataType::Text => "String".to_string(),
            DataType::Integer(crate::ir::IntSize::Small) => "Int".to_string(),
            DataType::Integer(crate::ir::IntSize::Standard) => "Int".to_string(),
            DataType::Integer(crate::ir::IntSize::Big) => "BigInt".to_string(),
            DataType::Float => "Float".to_string(),
            DataType::Decimal { .. } => "Decimal".to_string(),
            DataType::Boolean => "Boolean".to_string(),
            DataType::DateTime => "DateTime".to_string(),
            DataType::Json => "Json".to_string(),
            DataType::Uuid => "String @id @default(uuid())".to_string(),
            DataType::Enum { values, type_name } => {
                if let Some(enum_type) = type_name {
                    // Native PostgreSQL enum - use the type name
                    enum_type.to_string()
                } else {
                    capitalize_first(
                        values.first().map(|v| v.as_str()).unwrap_or("Status")
                    )
                }
            },
        }
    }

    fn file_extension(&self) -> &'static str {
        "prisma"
    }

    fn generate_model(&self, entity: &Entity) -> String {
        let mut output = String::new();

        let enum_fields: Vec<&Field> = entity
            .fields
            .iter()
            .filter(|f| matches!(f.data_type, DataType::Enum { values: _, type_name: _ }))
            .collect();

        if !enum_fields.is_empty() {
            for field in &enum_fields {
                if let DataType::Enum { values, type_name } = &field.data_type {
                    if type_name.is_none() {
                        // Only generate enum block if not using native PostgreSQL enum
                        let enum_name = capitalize_first(&field.name);
                        output.push_str(&format!("enum {} {{\n", enum_name));
                        for val in values {
                            let variant = val.to_uppercase();
                            output.push_str(&format!("  {}\n", variant));
                        }
                        output.push_str("}\n\n");
                    }
                }
            }
        }

        output.push_str(&format!("model {} {{\n", entity.name));

        let pk_fields: Vec<&Field> = entity
            .fields
            .iter()
            .filter(|f| f.constraints.is_primary_key)
            .collect();
        let has_composite_pk = pk_fields.len() > 1;

        for field in &entity.fields {
            let prisma_type = if field.constraints.is_primary_key && matches!(field.data_type, DataType::Uuid) && !has_composite_pk {
                "String @id @default(uuid())".to_string()
            } else if let DataType::Enum { values: _, type_name } = &field.data_type {
                if let Some(enum_type) = type_name {
                    // Native PostgreSQL enum - use the type name directly
                    if field.constraints.is_nullable {
                        format!("{}?", enum_type)
                    } else {
                        enum_type.to_string()
                    }
                } else {
                    let enum_name = capitalize_first(&field.name).to_string();
                    if field.constraints.is_nullable {
                        format!("{}?", enum_name)
                    } else {
                        enum_name
                    }
                }
            } else {
                let base_type = self.map_data_type(&field.data_type, field.constraints.is_nullable);
                if field.constraints.is_nullable {
                    format!("{}?", base_type)
                } else {
                    base_type
                }
            };

            output.push_str(&format!("  {} {}", field.name, prisma_type));

            if field.constraints.is_primary_key && !matches!(field.data_type, DataType::Uuid) && !has_composite_pk {
                output.push_str(" @id");
            }
            if field.constraints.is_unique && !field.constraints.is_primary_key {
                output.push_str(" @unique");
            }
            if field.constraints.is_indexed && !field.constraints.is_primary_key && !field.constraints.is_unique {
                output.push_str(" @index");
            }

            output.push('\n');
        }

        // Composite primary key
        if has_composite_pk {
            let pk_names: Vec<String> = pk_fields.iter().map(|f| f.name.clone()).collect();
            output.push_str(&format!("  @@id([{}])\n", pk_names.join(", ")));
        }

        output.push_str("}\n");
        output
    }

    fn generate_relations(&self, entity: &Entity, relations: &[Relation]) -> String {
        let mut output = String::new();
        let entity_relations: Vec<&Relation> = relations
            .iter()
            .filter(|r| r.source_entity_id == entity.id || r.target_entity_id == entity.id)
            .collect();

        for rel in entity_relations {
            let is_source = rel.source_entity_id == entity.id;
            let target_name = if is_source { rel.target_entity_id.clone() } else { rel.source_entity_id.clone() };
            let ref_field = if is_source { rel.source_field_name.clone() } else { rel.target_field_name.clone() };
            let target_field = if is_source { rel.target_field_name.clone() } else { rel.source_field_name.clone() };

            let target_struct_name = target_name.clone();
            let on_delete = rel.on_delete.as_ref().map(|a| self.map_referential_action(a)).unwrap_or_else(|| "Cascade".to_string());
            let on_update = rel.on_update.as_ref().map(|a| self.map_referential_action(a)).unwrap_or_else(|| "Cascade".to_string());

            match rel.relation_type {
                RelationType::OneToMany => {
                    if is_source {
                        output.push_str(&format!(
                            "  {} {}[] @relation(\"{}{}\")\n",
                            target_struct_name.to_lowercase(),
                            target_struct_name,
                            entity.name,
                            target_struct_name
                        ));
                    } else {
                        output.push_str(&format!(
                            "  {} {} @relation(\"{}{}\", fields: [{}], references: [{}], onDelete: {}, onUpdate: {})\n",
                            target_struct_name.to_lowercase(),
                            target_struct_name,
                            entity.name,
                            target_struct_name,
                            ref_field,
                            target_field,
                            on_delete,
                            on_update
                        ));
                    }
                }
                RelationType::OneToOne => {
                    if is_source {
                        output.push_str(&format!(
                            "  {} {} @relation(\"{}{}\", fields: [{}], references: [{}], onDelete: {}, onUpdate: {})\n",
                            target_struct_name.to_lowercase(),
                            target_struct_name,
                            entity.name,
                            target_struct_name,
                            target_field,
                            target_field,
                            on_delete,
                            on_update
                        ));
                    } else {
                        output.push_str(&format!(
                            "  {} {} @relation(\"{}{}\", fields: [{}], references: [{}], onDelete: {}, onUpdate: {})\n",
                            target_struct_name.to_lowercase(),
                            target_struct_name,
                            target_struct_name,
                            entity.name,
                            ref_field,
                            target_field,
                            on_delete,
                            on_update
                        ));
                    }
                }
                RelationType::ManyToMany => {
                    // For ManyToMany, TypeORM uses @ManyToMany decorator with join table
                    let source_name = if is_source {
                        entity.name.to_lowercase()
                    } else {
                        target_name.to_lowercase()
                    };
                    let other_name = if is_source {
                        target_name.to_lowercase()
                    } else {
                        entity.name.to_lowercase()
                    };
                    
                    // Determine junction table name (alphabetical)
                    let junction_name = if source_name < other_name {
                        format!("{}_{}", source_name, other_name)
                    } else {
                        format!("{}_{}", other_name, source_name)
                    };
                    
                    let source_fk = format!("{}_id", source_name);
                    let other_fk = format!("{}_id", other_name);
                    
                    // TypeORM uses @ManyToMany with @JoinTable
                    if is_source {
                        output.push_str(&format!(
                            "  @ManyToMany(() => {}, {{ cascade: true }})\n",
                            target_struct_name
                        ));
                        output.push_str(&format!(
                            "  @JoinTable({{\n",
                        ));
                        output.push_str(&format!(
                            "    name: '{}',\n",
                            junction_name
                        ));
                        output.push_str(&format!(
                            "    joinColumn: {{ name: '{}', referencedColumnName: '{}' }},\n",
                            source_fk, rel.source_field_name
                        ));
                        output.push_str(&format!(
                            "    inverseJoinColumn: {{ name: '{}', referencedColumnName: '{}' }}\n",
                            other_fk, rel.target_field_name
                        ));
                        output.push_str("  })\n");
                        output.push_str(&format!(
                            "  {}: {}[];\n\n",
                            target_name.to_lowercase() + "s",
                            target_struct_name
                        ));
                    } else {
                        output.push_str(&format!(
                            "  @ManyToMany(() => {}, {{ cascade: true }})\n",
                            target_struct_name
                        ));
                        output.push_str(&format!(
                            "  @JoinTable({{\n",
                        ));
                        output.push_str(&format!(
                            "    name: '{}',\n",
                            junction_name
                        ));
                        output.push_str(&format!(
                            "    joinColumn: {{ name: '{}', referencedColumnName: '{}' }},\n",
                            other_fk, rel.target_field_name
                        ));
                        output.push_str(&format!(
                            "    inverseJoinColumn: {{ name: '{}', referencedColumnName: '{}' }}\n",
                            source_fk, rel.source_field_name
                        ));
                        output.push_str("  })\n");
                        output.push_str(&format!(
                            "  {}: {}[];\n\n",
                            target_name.to_lowercase() + "s",
                            target_struct_name
                        ));
                    }
                }
            }
        }
        output
    }
}
pub struct TypeScriptTypeOrmDriver;

impl LanguageDriver for TypeScriptTypeOrmDriver {
    fn map_data_type(&self, data_type: &DataType, _is_nullable: bool) -> String {
        match data_type {
            DataType::String { .. } => "varchar".to_string(),
            DataType::Text => "text".to_string(),
            DataType::Integer(crate::ir::IntSize::Small) => "smallint".to_string(),
            DataType::Integer(crate::ir::IntSize::Standard) => "int".to_string(),
            DataType::Integer(crate::ir::IntSize::Big) => "bigint".to_string(),
            DataType::Float => "float".to_string(),
            DataType::Decimal { precision, scale } => format!("decimal({}, {})", precision, scale),
            DataType::Boolean => "boolean".to_string(),
            DataType::DateTime => "timestamp".to_string(),
            DataType::Json => "json".to_string(),
            DataType::Uuid => "uuid".to_string(),
            DataType::Enum { values, type_name } => {
                if let Some(enum_type) = type_name {
                    // Native PostgreSQL enum - use custom type
                    format!("enum(\"{}\")", enum_type)
                } else {
                    let enum_vals = values.iter().map(|v| format!("'{}'", v)).collect::<Vec<_>>().join(", ");
                    format!("enum({})", enum_vals)
                }
            },
        }
    }

    fn file_extension(&self) -> &'static str {
        "ts"
    }

    fn generate_model(&self, entity: &Entity) -> String {
        let mut output = String::new();

        let pk_fields: Vec<&Field> = entity
            .fields
            .iter()
            .filter(|f| f.constraints.is_primary_key)
            .collect();
        let has_composite_pk = pk_fields.len() > 1;

        let import = if has_composite_pk {
            "import { Entity, PrimaryColumn, Column } from 'typeorm';\n\n"
        } else {
            "import { Entity, PrimaryGeneratedColumn, Column } from 'typeorm';\n\n"
        };
        output.push_str(import);

        output.push_str(&format!("@Entity('{}')\n", entity.name.to_lowercase()));
        output.push_str(&format!("export class {} {{\n", entity.name));

        for field in &entity.fields {
            if field.constraints.is_primary_key {
                if has_composite_pk {
                    output.push_str("  @PrimaryColumn()\n");
                } else {
                    output.push_str("  @PrimaryGeneratedColumn('uuid')\n");
                }
                output.push_str(&format!("  {}: string;\n\n", field.name));
            } else {
                let col_type = self.map_data_type(&field.data_type, field.constraints.is_nullable);
                let mut col_options = String::new();

                if field.constraints.is_unique {
                    col_options.push_str("unique: true, ");
                }
                if field.constraints.is_indexed && !field.constraints.is_unique {
                    col_options.push_str("index: true, ");
                }
                if field.constraints.is_nullable {
                    col_options.push_str("nullable: true, ");
                }

                output.push_str("  @Column({\n");
                output.push_str(&format!("    type: '{}',\n", col_type));
                if !col_options.is_empty() {
                    output.push_str(&format!("    {}\n", col_options));
                }
                output.push_str("  })\n");

                let ts_type = match &field.data_type {
                    DataType::Boolean => "boolean".to_string(),
                    DataType::Integer(_) => "number".to_string(),
                    DataType::Float | DataType::Decimal { .. } => "number".to_string(),
                    DataType::Enum { values: _, type_name } => {
                        type_name.as_deref().unwrap_or(&format!("{}Enum", capitalize_first(&field.name))).to_string()
                    }
                    _ => "string".to_string(),
                };

                output.push_str(&format!("  {}: {};\n\n", field.name, ts_type));
            }
        }

        let enum_fields: Vec<&Field> = entity
            .fields
            .iter()
            .filter(|f| matches!(f.data_type, DataType::Enum { values: _, type_name: _ }))
            .collect();

        if !enum_fields.is_empty() {
            for field in &enum_fields {
                if let DataType::Enum { values, type_name } = &field.data_type {
                    if type_name.is_none() {
                        // Only generate TypeScript enum if not using native PostgreSQL enum
                        let enum_name = format!("{}Enum", capitalize_first(&field.name));
                        output.push_str(&format!("export enum {} {{\n", enum_name));
                        for val in values {
                            let const_name = val.to_uppercase();
                            output.push_str(&format!("    {} = '{}',\n", const_name, val));
                        }
                        output.push_str("}\n\n");
                    }
                }
            }
        }

        output.push_str("}\n");
        output
    }

    fn generate_relations(&self, entity: &Entity, relations: &[Relation]) -> String {
        let mut output = String::new();
        let entity_relations: Vec<&Relation> = relations
            .iter()
            .filter(|r| r.source_entity_id == entity.id || r.target_entity_id == entity.id)
            .collect();

        for rel in entity_relations {
            let is_source = rel.source_entity_id == entity.id;
            let target_name = if is_source { rel.target_entity_id.clone() } else { rel.source_entity_id.clone() };
            let ref_field = if is_source { rel.source_field_name.clone() } else { rel.target_field_name.clone() };
            let target_field = if is_source { rel.target_field_name.clone() } else { rel.source_field_name.clone() };

            let target_struct_name = target_name;
            let on_delete = rel.on_delete.as_ref().map(|a| self.map_referential_action(a)).unwrap_or_else(|| "CASCADE".to_string());
            let on_update = rel.on_update.as_ref().map(|a| self.map_referential_action(a)).unwrap_or_else(|| "CASCADE".to_string());

            match rel.relation_type {
                RelationType::OneToMany => {
                    if is_source {
                        output.push_str(&format!(
                            "  @OneToMany(() => {}, {} => {}.{})\n",
                            target_struct_name,
                            target_struct_name.to_lowercase(),
                            target_struct_name.to_lowercase(),
                            ref_field
                        ));
                    } else {
                        output.push_str(&format!(
                            "  @ManyToOne(() => {}, {} => {}.{})\n",
                            target_struct_name,
                            target_struct_name.to_lowercase(),
                            target_struct_name.to_lowercase(),
                            target_field
                        ));
                        output.push_str(&format!(
                            "  @JoinColumn({{ name: '{}', referencedColumnName: '{}', onDelete: '{}', onUpdate: '{}' }})\n",
                            ref_field, target_field, on_delete, on_update
                        ));
                    }
                }
                RelationType::OneToOne => {
                    if is_source {
                        output.push_str(&format!(
                            "  @OneToOne(() => {}, {{ nullable: true }})\n",
                            target_struct_name
                        ));
                        output.push_str(&format!(
                            "  @JoinColumn({{ name: '{}', referencedColumnName: '{}', onDelete: '{}', onUpdate: '{}' }})\n",
                            target_field, target_field, on_delete, on_update
                        ));
                    } else {
                        output.push_str(&format!(
                            "  @OneToOne(() => {}, {{ nullable: true }})\n",
                            target_struct_name
                        ));
                        output.push_str(&format!(
                            "  @JoinColumn({{ name: '{}', referencedColumnName: '{}', onDelete: '{}', onUpdate: '{}' }})\n",
                            ref_field, target_field, on_delete, on_update
                        ));
                    }
                }
                RelationType::ManyToMany => {
                    // Many-to-many via @ManyToMany
                }
            }
        }
        output
    }
}

pub struct TypeScriptValkyrinDriver;

impl LanguageDriver for TypeScriptValkyrinDriver {
    fn map_data_type(&self, data_type: &DataType, is_nullable: bool) -> String {
        let base_type = match data_type {
            DataType::String { .. } | DataType::Text => "string".to_string(),
            DataType::Integer(IntSize::Small) | DataType::Integer(IntSize::Standard) => "number".to_string(),
            DataType::Integer(IntSize::Big) => "bigint".to_string(),
            DataType::Float | DataType::Decimal { .. } => "number".to_string(),
            DataType::Boolean => "boolean".to_string(),
            DataType::DateTime => "Date".to_string(),
            DataType::Json => "Record<string, unknown>".to_string(),
            DataType::Uuid => "string".to_string(),
            DataType::Enum { values, type_name } => {
                if let Some(type_name) = type_name {
                    format!("$Enums.{}", type_name)
                } else {
                    values.iter().map(|v| format!("'{}'", v)).collect::<Vec<_>>().join(" | ")
                }
            }
        };
        if is_nullable {
            format!("{} | null", base_type)
        } else {
            base_type
        }
    }

    fn file_extension(&self) -> &'static str {
        "ts"
    }

    fn generate_model(&self, _entity: &Entity) -> String {
        String::new()
    }

    fn generate_full_client(&self, _graph: &EntityGraph) -> ValkyrinResult<Option<Vec<(String, String)>>> {
        Ok(Some(vec![]))
    }
}

fn to_camel_case(s: &str) -> String {
    let mut result = String::new();
    let mut capitalize_next = false;
    for c in s.chars() {
        if c == '_' {
            capitalize_next = true;
        } else if capitalize_next {
            result.push(c.to_uppercase().next().unwrap());
            capitalize_next = false;
        } else {
            result.push(c);
        }
    }
    result
}

#[derive(Copy, Clone)]
pub enum TargetBackend {
    GoGorm,
    GoEnt,
    PythonSqlAlchemy,
    PythonSqlModel,
    RustDiesel,
    RustSeaOrm,
    JavaScriptSequelize,
    JavaScriptTypeOrm,
    TypeScriptPrisma,
    TypeScriptTypeOrm,
    TypeScriptValkyrin,
}

pub fn get_driver(backend: TargetBackend) -> Result<Box<dyn LanguageDriver>> {
    match backend {
        TargetBackend::GoGorm => Ok(Box::new(GoGormDriver)),
        TargetBackend::GoEnt => Ok(Box::new(GoEntDriver)),
        TargetBackend::PythonSqlModel => Ok(Box::new(PythonSqlModelDriver)),
        TargetBackend::PythonSqlAlchemy => Ok(Box::new(PythonSqlAlchemyDriver)),
        TargetBackend::RustDiesel => Ok(Box::new(RustDieselDriver)),
        TargetBackend::RustSeaOrm => Ok(Box::new(RustSeaOrmDriver)),
        TargetBackend::JavaScriptSequelize => Ok(Box::new(JavaScriptSequelizeDriver)),
        TargetBackend::JavaScriptTypeOrm => Ok(Box::new(JavaScriptTypeOrmDriver)),
        TargetBackend::TypeScriptPrisma => Ok(Box::new(TypeScriptPrismaDriver)),
        TargetBackend::TypeScriptTypeOrm => Ok(Box::new(TypeScriptTypeOrmDriver)),
        TargetBackend::TypeScriptValkyrin => Ok(Box::new(TypeScriptValkyrinDriver)),
    }
}
