// valkyrin-core/src/codegen.rs
use crate::ir::{DataType, Entity, EntityGraph, Field, IntSize, Relation, RelationType, ReferentialAction};
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
    /// Default implementation does nothing (used by TypeScriptValkyrinDriver).
    fn generate_full_client(&self, _graph: &EntityGraph, _output_dir: &str) -> Result<()> {
        Ok(())
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

// Helper struct for relation info
#[derive(Clone, Debug)]
struct RelationInfo {
    target_entity_name: String,
    target_field_name: String,
    source_field_name: String,
    relation_type: RelationType,
    is_source_side: bool,
}

pub struct TypeScriptValkyrinDriver;

impl TypeScriptValkyrinDriver {
    /// Sanitize an identifier: ensure it's not a reserved keyword, append `_` if it is
    fn sanitize_name(&self, name: &str) -> String {
        let reserved = &[
            "break", "case", "catch", "class", "const", "continue", "debugger", "default",
            "delete", "do", "else", "enum", "export", "extends", "false", "finally", "for",
            "function", "if", "import", "in", "instanceof", "new", "null", "return",
            "super", "switch", "this", "throw", "true", "try", "typeof", "var", "void",
            "while", "with",
            "valkyrinclient", "valkyrin", "delegate", "payload", "extensions",
        ];
        if reserved.iter().any(|r| *r == name.to_lowercase()) {
            format!("{}_", name)
        } else {
            name.to_string()
        }
    }

    /// Get TypeScript type for a field
    fn get_field_ts_type(&self, field: &Field) -> String {
        self.map_data_type(&field.data_type, field.constraints.is_nullable)
    }

    /// Generate Payload type for an entity
    fn generate_payload_type(
        &self,
        entity_name: &str,
        scalars: &[&Field],
        objects: &[&Field],
        composites: &[&Field],
        _graph: &EntityGraph,
    ) -> String {
        let mut output = String::new();
        
        // Scalars section
        let scalar_fields: Vec<String> = scalars.iter()
            .map(|f| format!("    {}: {};", f.name, self.get_field_ts_type(f)))
            .collect();
        
        // Composites section
        let composite_fields: Vec<String> = composites.iter()
            .map(|f| format!("    {}: {};", f.name, self.get_field_ts_type(f)))
            .collect();
        
        // Objects (relations) section - recursive payload types
        let object_fields: Vec<String> = objects.iter()
            .map(|f| {
                let relation_info = self.build_relation_map(_graph, &Entity {
                    id: String::new(),
                    name: entity_name.to_string(),
                    fields: vec![],
                }).get(&f.name).cloned();
                
                if let Some(rel) = relation_info {
                    let target_name = &rel.target_entity_name;
                    match rel.relation_type {
                        crate::ir::RelationType::OneToMany | crate::ir::RelationType::ManyToMany => {
                            format!("    {}: {}Payload[];", f.name, target_name)
                        }
                        crate::ir::RelationType::OneToOne => {
                            format!("    {}: {}Payload | null;", f.name, target_name)
                        }
                    }
                } else {
                    format!("    {}: any;", f.name)
                }
            })
            .collect();
        
        output.push_str(&format!("export type {}Payload<ExtArgs extends {{}} = {{", entity_name));
        output.push_str("\n  scalars: {\n");
        output.push_str(&scalar_fields.join("\n"));
        output.push_str("\n  };\n");
        
        if !composites.is_empty() {
            output.push_str("  composites: {\n");
            output.push_str(&composite_fields.join("\n"));
            output.push_str("\n  };\n");
        }
        
        if !objects.is_empty() {
            output.push_str("  objects: {\n");
            output.push_str(&object_fields.join("\n"));
            output.push_str("\n  };\n");
        }
        
        output.push_str("};");
        output
    }

    /// Build a map of relation fields for an entity
    fn build_relation_map(&self, graph: &EntityGraph, entity: &Entity) -> std::collections::HashMap<String, RelationInfo> {
        let mut map = std::collections::HashMap::new();
        
        for rel in &graph.relations {
            let is_source = rel.source_entity_id == entity.id;
            let is_target = rel.target_entity_id == entity.id;
            
            if is_source || is_target {
                let (target_entity_id, target_field_name, source_field_name, relation_type, is_source_side) = if is_source {
                    (&rel.target_entity_id, &rel.target_field_name, &rel.source_field_name, rel.relation_type.clone(), true)
                } else {
                    (&rel.source_entity_id, &rel.source_field_name, &rel.target_field_name, rel.relation_type.clone(), false)
                };
                
                // Find target entity
                if let Some(target_entity) = graph.entities.iter().find(|e| e.id == *target_entity_id) {
                    let field_name = if is_source_side { source_field_name } else { target_field_name };
                    map.insert(field_name.clone(), RelationInfo {
                        target_entity_name: target_entity.name.clone(),
                        target_field_name: target_field_name.clone(),
                        source_field_name: source_field_name.clone(),
                        relation_type,
                        is_source_side,
                    });
                }
            }
        }
        
        map
    }

    /// Partition fields into scalars, objects (relations), and composites (embedded)
    fn partition_fields<'a>(
        &self,
        entity: &'a Entity,
        relations: &std::collections::HashMap<String, RelationInfo>,
        _graph: &EntityGraph,
    ) -> (Vec<&'a Field>, Vec<&'a Field>, Vec<&'a Field>) {
        let mut scalars = Vec::new();
        let mut objects = Vec::new();
        let mut composites = Vec::new();
        
        for field in &entity.fields {
            // Check if this field is a relation
            if relations.contains_key(&field.name) {
                // This is a relation field
                objects.push(field);
            } else if field.is_composite || matches!(field.data_type, DataType::Json) {
                // This is a composite/embedded field
                composites.push(field);
            } else {
                // This is a scalar field (including enums)
                scalars.push(field);
            }
        }
        
        (scalars, objects, composites)
    }

    /// Generate Select type for an entity
    fn generate_select_type(
        &self,
        entity_name: &str,
        scalars: &[&Field],
        objects: &[&Field],
        composites: &[&Field],
        graph: &EntityGraph,
    ) -> String {
        let mut output = String::new();
        
        // Scalars: boolean
        let scalar_fields: Vec<String> = scalars.iter()
            .map(|f| format!("    {}?: boolean;", f.name))
            .collect();
        
        // Composites: boolean | nested select
        let composite_fields: Vec<String> = composites.iter()
            .map(|f| format!("    {}?: boolean | {}Select<ExtArgs>;", f.name, entity_name))
            .collect();
        
        // Objects (relations): boolean | nested select
        let object_fields: Vec<String> = objects.iter()
            .map(|f| {
                let relations = self.build_relation_map(graph, &Entity {
                    id: String::new(),
                    name: entity_name.to_string(),
                    fields: vec![],
                });
                if let Some(rel) = relations.get(&f.name) {
                    let target_name = &rel.target_entity_name;
                    match rel.relation_type {
                        crate::ir::RelationType::OneToMany | crate::ir::RelationType::ManyToMany => {
                            format!("    {}?: boolean | {}Select<ExtArgs>;", f.name, target_name)
                        }
                        crate::ir::RelationType::OneToOne => {
                            format!("    {}?: boolean | {}Select<ExtArgs>;", f.name, target_name)
                        }
                    }
                } else {
                    format!("    {}?: boolean;", f.name)
                }
            })
            .collect();
        
        output.push_str(&format!("export type {}Select<ExtArgs extends {{}} = {{", entity_name));
        
        if !scalar_fields.is_empty() {
            output.push_str("\n  scalars?: {\n");
            output.push_str(&scalar_fields.join("\n"));
            output.push_str("\n  };\n");
        }
        
        if !composite_fields.is_empty() {
            output.push_str("  composites?: {\n");
            output.push_str(&composite_fields.join("\n"));
            output.push_str("\n  };\n");
        }
        
        if !object_fields.is_empty() {
            output.push_str("  objects?: {\n");
            output.push_str(&object_fields.join("\n"));
            output.push_str("\n  };\n");
        }
        
        output.push_str("};");
        output
    }

    /// Generate Include type for an entity
    fn generate_include_type(
        &self,
        entity_name: &str,
        objects: &[&Field],
        graph: &EntityGraph,
    ) -> String {
        let mut output = String::new();
        
        // Only relations can be included
        let object_fields: Vec<String> = objects.iter()
            .map(|f| {
                let relations = self.build_relation_map(graph, &Entity {
                    id: String::new(),
                    name: entity_name.to_string(),
                    fields: vec![],
                });
                if let Some(rel) = relations.get(&f.name) {
                    let target_name = &rel.target_entity_name;
                    match rel.relation_type {
                        crate::ir::RelationType::OneToMany | crate::ir::RelationType::ManyToMany => {
                            format!("    {}?: boolean | {}Include<ExtArgs>;", f.name, target_name)
                        }
                        crate::ir::RelationType::OneToOne => {
                            format!("    {}?: boolean | {}Include<ExtArgs>;", f.name, target_name)
                        }
                    }
                } else {
                    format!("    {}?: boolean;", f.name)
                }
            })
            .collect();
        
        if object_fields.is_empty() {
            return format!("export type {}Include<ExtArgs extends {{}} = {{}};", entity_name);
        }
        
        output.push_str(&format!("export type {}Include<ExtArgs extends {{}} = {{", entity_name));
        output.push_str("\n  objects?: {\n");
        output.push_str(&object_fields.join("\n"));
        output.push_str("\n  };\n");
        output.push_str("};");
        output
    }

    /// Generate Omit type for an entity
    fn generate_omit_type(
        &self,
        entity_name: &str,
        scalars: &[&Field],
        composites: &[&Field],
    ) -> String {
        let mut output = String::new();
        
        // Only scalars and composites can be omitted
        let mut omit_fields: Vec<String> = Vec::new();
        
        for f in scalars {
            omit_fields.push(format!("    {}?: boolean;", f.name));
        }
        for f in composites {
            omit_fields.push(format!("    {}?: boolean;", f.name));
        }
        
        if omit_fields.is_empty() {
            return format!("export type {}Omit<ExtArgs extends {{}} = {{}};", entity_name);
        }
        
        output.push_str(&format!("export type {}Omit<ExtArgs extends {{}} = {{", entity_name));
        output.push_str("\n  scalars?: {\n");
        output.push_str(&omit_fields.join("\n"));
        output.push_str("\n  };\n");
        output.push_str("};");
        output
    }

    /// Get the base operator type for a field based on its data type
    fn get_field_operators(&self, field: &Field) -> String {
        match &field.data_type {
            DataType::String { .. } | DataType::Text => {
                r#"{ equals?: string; notEquals?: string; in?: string[]; notIn?: string[]; lt?: string; lte?: string; gt?: string; gte?: string; contains?: string; startsWith?: string; endsWith?: string; isNull?: boolean; }"#.to_string()
            }
            DataType::Integer(_) | DataType::Float | DataType::Decimal { .. } => {
                r#"{ equals?: number; notEquals?: number; in?: number[]; notIn?: number[]; lt?: number; lte?: number; gt?: number; gte?: number; isNull?: boolean; }"#.to_string()
            }
            DataType::Boolean => {
                r#"{ equals?: boolean; notEquals?: boolean; isNull?: boolean; }"#.to_string()
            }
            DataType::DateTime => {
                r#"{ equals?: Date; notEquals?: Date; in?: Date[]; notIn?: Date[]; lt?: Date; lte?: Date; gt?: Date; gte?: Date; isNull?: boolean; }"#.to_string()
            }
            DataType::Uuid => {
                r#"{ equals?: string; notEquals?: string; in?: string[]; notIn?: string[]; isNull?: boolean; }"#.to_string()
            }
            DataType::Json => {
                r#"{ equals?: Record<string, unknown>; notEquals?: Record<string, unknown>; isNull?: boolean; }"#.to_string()
            }
            DataType::Enum { .. } => {
                let enum_type = self.map_data_type(&field.data_type, false);
                format!("{{ equals?: {}; notEquals?: {}; in?: {}[]; notIn?: {}[]; isNull?: boolean; }}", enum_type, enum_type, enum_type, enum_type)
            }
        }
    }

    /// Generate Where type for an entity (recursive with AND/OR/NOT)
    fn generate_where_type(
        &self,
        entity_name: &str,
        scalars: &[&Field],
        objects: &[&Field],
        graph: &EntityGraph,
    ) -> String {
        let mut output = String::new();
        
        // Scalar field conditions
        let scalar_conditions: Vec<String> = scalars.iter()
            .map(|f| format!("    {}?: {};", f.name, self.get_field_operators(f)))
            .collect();
        
        // Relation field conditions (nested where)
        let object_conditions: Vec<String> = objects.iter()
            .map(|f| {
                let relations = self.build_relation_map(graph, &Entity {
                    id: String::new(),
                    name: entity_name.to_string(),
                    fields: vec![],
                });
                if let Some(rel) = relations.get(&f.name) {
                    let target_name = &rel.target_entity_name;
                    match rel.relation_type {
                        crate::ir::RelationType::OneToMany | crate::ir::RelationType::ManyToMany => {
                            format!("    {}?: {{ some?: {}WhereInput<ExtArgs>; none?: {}WhereInput<ExtArgs>; every?: {}WhereInput<ExtArgs>; }};", f.name, target_name, target_name, target_name)
                        }
                        crate::ir::RelationType::OneToOne => {
                            format!("    {}?: {{ is?: {}WhereInput<ExtArgs>; isNull?: boolean; }};", f.name, target_name)
                        }
                    }
                } else {
                    format!("    {}?: any;", f.name)
                }
            })
            .collect();
        
        let type_name = format!("{}WhereInput<ExtArgs>", entity_name);
        
        output.push_str(&format!("export type {} = {{", type_name));
        output.push_str(&format!("\n  AND?: {}WhereInput<ExtArgs>[];\n", entity_name));
        output.push_str(&format!("  OR?: {}WhereInput<ExtArgs>[];\n", entity_name));
        output.push_str(&format!("  NOT?: {}WhereInput<ExtArgs> | {}WhereInput<ExtArgs>[];\n", entity_name, entity_name));
        
        if !scalar_conditions.is_empty() {
            output.push_str("  scalars?: {\n");
            output.push_str(&scalar_conditions.join("\n"));
            output.push_str("\n  };\n");
        }
        
        if !object_conditions.is_empty() {
            output.push_str("  objects?: {\n");
            output.push_str(&object_conditions.join("\n"));
            output.push_str("\n  };\n");
        }
        
        output.push_str("};");
        output
    }

    /// Generate OrderBy type for an entity
    fn generate_orderby_type(
        &self,
        entity_name: &str,
        scalars: &[&Field],
    ) -> String {
        let mut output = String::new();
        
        let scalar_fields: Vec<String> = scalars.iter()
            .map(|f| format!("    {}?: 'asc' | 'desc';", f.name))
            .collect();
        
        if scalar_fields.is_empty() {
            return format!("export type {}OrderByInput<ExtArgs> = {{}};", entity_name);
        }
        
        output.push_str(&format!("export type {}OrderByInput<ExtArgs> = {{", entity_name));
        output.push_str("\n  scalars?: {\n");
        output.push_str(&scalar_fields.join("\n"));
        output.push_str("\n  };\n");
        output.push_str("};");
        output
    }

    /// Generate FindUnique args type
    fn generate_find_unique_args(&self, entity_name: &str, scalars: &[&Field]) -> String {
        let pk_fields: Vec<String> = scalars.iter()
            .filter(|f| f.constraints.is_primary_key)
            .map(|f| f.name.clone())
            .collect();
        
        let where_type = if pk_fields.len() == 1 {
            format!("{}WhereUniqueInput<ExtArgs>", entity_name)
        } else {
            format!("{}WhereUniqueInput<ExtArgs>", entity_name)
        };
        
        format!(r#"export type {}FindUniqueArgs<ExtArgs extends {{}} = {{
  where: {};
  select?: {}Select<ExtArgs>;
  include?: {}Include<ExtArgs>;
  omit?: {}Omit<ExtArgs>;
}};"#, entity_name, where_type, entity_name, entity_name, entity_name)
    }

    /// Generate FindMany args type
    fn generate_find_many_args(&self, entity_name: &str, scalars: &[&Field], objects: &[&Field], graph: &EntityGraph) -> String {
        let _ = (scalars, objects, graph); // suppress unused warnings for now
        format!(r#"export type {}FindManyArgs<ExtArgs extends {{}} = {{
  where?: {}WhereInput<ExtArgs>;
  orderBy?: {}OrderByInput<ExtArgs> | {}OrderByInput<ExtArgs>[];
  take?: number;
  skip?: number;
  select?: {}Select<ExtArgs>;
  include?: {}Include<ExtArgs>;
  omit?: {}Omit<ExtArgs>;
}};"#, entity_name, entity_name, entity_name, entity_name, entity_name, entity_name, entity_name)
    }

    /// Generate Create args type
    fn generate_create_args(&self, entity_name: &str, scalars: &[&Field], objects: &[&Field], composites: &[&Field], graph: &EntityGraph) -> String {
        let _ = (scalars, objects, composites, graph);
        format!(r#"export type {}CreateArgs<ExtArgs extends {{}} = {{
  data: {}CreateInput<ExtArgs>;
  select?: {}Select<ExtArgs>;
  include?: {}Include<ExtArgs>;
  omit?: {}Omit<ExtArgs>;
}};"#, entity_name, entity_name, entity_name, entity_name, entity_name)
    }

    /// Generate Update args type
    fn generate_update_args(&self, entity_name: &str, scalars: &[&Field], objects: &[&Field], composites: &[&Field], graph: &EntityGraph) -> String {
        let _ = (scalars, objects, composites, graph);
        format!(r#"export type {}UpdateArgs<ExtArgs extends {{}} = {{
  where: {}WhereUniqueInput<ExtArgs>;
  data: {}UpdateInput<ExtArgs>;
  select?: {}Select<ExtArgs>;
  include?: {}Include<ExtArgs>;
  omit?: {}Omit<ExtArgs>;
}};"#, entity_name, entity_name, entity_name, entity_name, entity_name, entity_name)
    }

    /// Generate Delete args type
    fn generate_delete_args(&self, entity_name: &str, scalars: &[&Field]) -> String {
        let _ = scalars;
        format!(r#"export type {}DeleteArgs<ExtArgs extends {{}} = {{
  where: {}WhereUniqueInput<ExtArgs>;
  select?: {}Select<ExtArgs>;
  include?: {}Include<ExtArgs>;
  omit?: {}Omit<ExtArgs>;
}};"#, entity_name, entity_name, entity_name, entity_name, entity_name)
    }

    /// Generate Upsert args type
    fn generate_upsert_args(&self, entity_name: &str, scalars: &[&Field], objects: &[&Field], composites: &[&Field], graph: &EntityGraph) -> String {
        let _ = (scalars, objects, composites, graph);
        format!(r#"export type {}UpsertArgs<ExtArgs extends {{}} = {{
  where: {}WhereUniqueInput<ExtArgs>;
  create: {}CreateInput<ExtArgs>;
  update: {}UpdateInput<ExtArgs>;
  select?: {}Select<ExtArgs>;
  include?: {}Include<ExtArgs>;
  omit?: {}Omit<ExtArgs>;
}};"#, entity_name, entity_name, entity_name, entity_name, entity_name, entity_name, entity_name)
    }

    /// Generate Aggregate args type
    fn generate_aggregate_args(&self, entity_name: &str, scalars: &[&Field]) -> String {
        let _ = scalars;
        format!(r#"export type {}AggregateArgs<ExtArgs extends {{}} = {{
  where?: {}WhereInput<ExtArgs>;
  orderBy?: {}OrderByInput<ExtArgs> | {}OrderByInput<ExtArgs>[];
  take?: number;
  skip?: number;
  _count?: boolean | {}CountAggregateInput;
  _avg?: {}AvgAggregateInput;
  _sum?: {}SumAggregateInput;
  _min?: {}MinAggregateInput;
  _max?: {}MaxAggregateInput;
}};"#, entity_name, entity_name, entity_name, entity_name, entity_name, entity_name, entity_name, entity_name, entity_name)
    }

    /// Generate GroupBy args type
    fn generate_group_by_args(&self, entity_name: &str, scalars: &[&Field]) -> String {
        let _ = scalars;
        format!(r#"export type {}GroupByArgs<ExtArgs extends {{}} = {{
  where?: {}WhereInput<ExtArgs>;
  orderBy?: {}OrderByInput<ExtArgs> | {}OrderByInput<ExtArgs>[];
  take?: number;
  skip?: number;
  by: {}GroupByFields[];
  _count?: {}CountAggregateInput;
  _avg?: {}AvgAggregateInput;
  _sum?: {}SumAggregateInput;
  _min?: {}MinAggregateInput;
  _max?: {}MaxAggregateInput;
}};"#, entity_name, entity_name, entity_name, entity_name, entity_name, entity_name, entity_name, entity_name, entity_name, entity_name)
    }

    /// Generate WhereUniqueInput type (for findUnique, update, delete, upsert)
    fn generate_where_unique_input(&self, entity_name: &str, scalars: &[&Field]) -> String {
        let pk_fields: Vec<String> = scalars.iter()
            .filter(|f| f.constraints.is_primary_key)
            .map(|f| format!("    {}?: {};", f.name, self.get_field_ts_type(f)))
            .collect();
        
        if pk_fields.is_empty() {
            return format!("export type {}WhereUniqueInput<ExtArgs> = {{}};", entity_name);
        }
        
        format!("export type {}WhereUniqueInput<ExtArgs> = {{\n{}\n}};", entity_name, pk_fields.join("\n"))
    }

    /// Generate CreateInput type
    fn generate_create_input(&self, entity_name: &str, scalars: &[&Field], objects: &[&Field], composites: &[&Field], graph: &EntityGraph) -> String {
        let mut output = String::new();
        
        // Scalar fields (required if not nullable, optional if nullable)
        let scalar_fields: Vec<String> = scalars.iter()
            .filter(|f| !f.constraints.is_primary_key || f.constraints.default_value.is_some())
            .map(|f| {
                let optional = if f.constraints.is_nullable || f.constraints.default_value.is_some() { "?" } else { "" };
                format!("    {}{}: {};", f.name, optional, self.get_field_ts_type(f))
            })
            .collect();
        
        // Composite fields
        let composite_fields: Vec<String> = composites.iter()
            .map(|f| {
                let optional = if f.constraints.is_nullable { "?" } else { "" };
                format!("    {}{}: {};", f.name, optional, self.get_field_ts_type(f))
            })
            .collect();
        
        // Object/relation fields (nested creates)
        let object_fields: Vec<String> = objects.iter()
            .map(|f| {
                let relations = self.build_relation_map(graph, &Entity {
                    id: String::new(),
                    name: entity_name.to_string(),
                    fields: vec![],
                });
                if let Some(rel) = relations.get(&f.name) {
                    let target_name = &rel.target_entity_name;
                    match rel.relation_type {
                        crate::ir::RelationType::OneToMany | crate::ir::RelationType::ManyToMany => {
                            format!("    {}?: {{ create?: {}CreateInput<ExtArgs>[]; connect?: {}WhereUniqueInput<ExtArgs>[]; }};", f.name, target_name, target_name)
                        }
                        crate::ir::RelationType::OneToOne => {
                            format!("    {}?: {{ create?: {}CreateInput<ExtArgs>; connect?: {}WhereUniqueInput<ExtArgs>; }};", f.name, target_name, target_name)
                        }
                    }
                } else {
                    format!("    {}?: any;", f.name)
                }
            })
            .collect();
        
        output.push_str(&format!("export type {}CreateInput<ExtArgs> = {{", entity_name));
        
        if !scalar_fields.is_empty() {
            output.push_str("\n  scalars: {\n");
            output.push_str(&scalar_fields.join("\n"));
            output.push_str("\n  };\n");
        }
        
        if !composite_fields.is_empty() {
            output.push_str("  composites: {\n");
            output.push_str(&composite_fields.join("\n"));
            output.push_str("\n  };\n");
        }
        
        if !object_fields.is_empty() {
            output.push_str("  objects: {\n");
            output.push_str(&object_fields.join("\n"));
            output.push_str("\n  };\n");
        }
        
        output.push_str("};");
        output
    }

    /// Generate UpdateInput type
    fn generate_update_input(&self, entity_name: &str, scalars: &[&Field], objects: &[&Field], composites: &[&Field], graph: &EntityGraph) -> String {
        let mut output = String::new();
        
        // Scalar fields (all optional for update)
        let scalar_fields: Vec<String> = scalars.iter()
            .filter(|f| !f.constraints.is_primary_key)
            .map(|f| format!("    {}?: {};", f.name, self.get_field_ts_type(f)))
            .collect();
        
        // Composite fields (all optional for update)
        let composite_fields: Vec<String> = composites.iter()
            .map(|f| format!("    {}?: {};", f.name, self.get_field_ts_type(f)))
            .collect();
        
        // Object/relation fields (nested updates)
        let object_fields: Vec<String> = objects.iter()
            .map(|f| {
                let relations = self.build_relation_map(graph, &Entity {
                    id: String::new(),
                    name: entity_name.to_string(),
                    fields: vec![],
                });
                if let Some(rel) = relations.get(&f.name) {
                    let target_name = &rel.target_entity_name;
                    match rel.relation_type {
                        crate::ir::RelationType::OneToMany | crate::ir::RelationType::ManyToMany => {
                            format!("    {}?: {{ create?: {}CreateInput<ExtArgs>[]; connect?: {}WhereUniqueInput<ExtArgs>[]; disconnect?: {}WhereUniqueInput<ExtArgs>[]; delete?: {}WhereUniqueInput<ExtArgs>[]; update?: {}UpdateInput<ExtArgs>[]; set?: {}WhereUniqueInput<ExtArgs>[]; }};", f.name, target_name, target_name, target_name, target_name, target_name, target_name)
                        }
                        crate::ir::RelationType::OneToOne => {
                            format!("    {}?: {{ create?: {}CreateInput<ExtArgs>; connect?: {}WhereUniqueInput<ExtArgs>; disconnect?: boolean; delete?: boolean; update?: {}UpdateInput<ExtArgs>; }};", f.name, target_name, target_name, target_name)
                        }
                    }
                } else {
                    format!("    {}?: any;", f.name)
                }
            })
            .collect();
        
        output.push_str(&format!("export type {}UpdateInput<ExtArgs> = {{", entity_name));
        
        if !scalar_fields.is_empty() {
            output.push_str("\n  scalars: {\n");
            output.push_str(&scalar_fields.join("\n"));
            output.push_str("\n  };\n");
        }
        
        if !composite_fields.is_empty() {
            output.push_str("  composites: {\n");
            output.push_str(&composite_fields.join("\n"));
            output.push_str("\n  };\n");
        }
        
        if !object_fields.is_empty() {
            output.push_str("  objects: {\n");
            output.push_str(&object_fields.join("\n"));
            output.push_str("\n  };\n");
        }
        
        output.push_str("};");
        output
    }

    /// Generate aggregate input types
    fn generate_aggregate_inputs(&self, entity_name: &str, scalars: &[&Field]) -> String {
        let mut output = String::new();
        
        // Count aggregate - all fields
        let count_fields: Vec<String> = scalars.iter()
            .map(|f| format!("    {}?: boolean;", f.name))
            .collect();
        
        // Avg/Sum/Min/Max - only numeric fields
        let numeric_fields: Vec<String> = scalars.iter()
            .filter(|f| matches!(f.data_type, DataType::Integer(_) | DataType::Float | DataType::Decimal { .. }))
            .map(|f| format!("    {}?: boolean;", f.name))
            .collect();
        
        output.push_str(&format!("export type {}CountAggregateInput = {{\n  scalars?: {{\n{}\n  }};\n}};\n\n", entity_name, count_fields.join("\n")));
        
        if !numeric_fields.is_empty() {
            output.push_str(&format!("export type {}AvgAggregateInput = {{\n  scalars?: {{\n{}\n  }};\n}};\n\n", entity_name, numeric_fields.join("\n")));
            output.push_str(&format!("export type {}SumAggregateInput = {{\n  scalars?: {{\n{}\n  }};\n}};\n\n", entity_name, numeric_fields.join("\n")));
            output.push_str(&format!("export type {}MinAggregateInput = {{\n  scalars?: {{\n{}\n  }};\n}};\n\n", entity_name, numeric_fields.join("\n")));
            output.push_str(&format!("export type {}MaxAggregateInput = {{\n  scalars?: {{\n{}\n  }};\n}};\n\n", entity_name, numeric_fields.join("\n")));
        } else {
            output.push_str(&format!("export type {}AvgAggregateInput = {{}};\n\n", entity_name));
            output.push_str(&format!("export type {}SumAggregateInput = {{}};\n\n", entity_name));
            output.push_str(&format!("export type {}MinAggregateInput = {{}};\n\n", entity_name));
            output.push_str(&format!("export type {}MaxAggregateInput = {{}};\n\n", entity_name));
        }
        
        // GroupBy fields (all scalars)
        let groupby_fields: Vec<String> = scalars.iter()
            .map(|f| format!("  {}", f.name))
            .collect();
        output.push_str(&format!("export type {}GroupByFields = {};", entity_name, groupby_fields.join(" | ")));
        
        output
    }
}

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
        unimplemented!("TypeScriptValkyrinDriver uses generate_full_client instead")
    }

    fn generate_full_client(&self, graph: &EntityGraph, output_dir: &str) -> Result<()> {
        use std::fs;
        use std::path::Path;

        let enums_path = Path::new(output_dir).join("enums.ts");
        let enums_content = self.generate_enums(graph);
        fs::write(&enums_path, enums_content)?;

        let types_path = Path::new(output_dir).join("types.ts");
        let types_content = self.generate_types();
        fs::write(&types_path, types_content)?;

        let operations_path = Path::new(output_dir).join("operations.ts");
        let operations_content = self.generate_operations(graph);
        fs::write(&operations_path, operations_content)?;

        let client_path = Path::new(output_dir).join("client.ts");
        let client_content = self.generate_client(graph);
        fs::write(&client_path, client_content)?;

        let index_path = Path::new(output_dir).join("index.ts");
        let index_content = self.generate_index(graph);
        fs::write(&index_path, index_content)?;

        Ok(())
    }

    fn generate_enums(&self, graph: &EntityGraph) -> String {
        let mut enums = Vec::new();
        let mut seen = std::collections::HashSet::new();

        for entity in &graph.entities {
            for field in &entity.fields {
                if let DataType::Enum { values, type_name } = &field.data_type {
                    let enum_key = type_name.as_ref().map(|s| s.as_str()).unwrap_or(&field.name);
                    if seen.insert(enum_key.to_string()) {
                        let enum_name = type_name.clone().unwrap_or_else(|| capitalize_first(&field.name));
                        let variants = values.iter().map(|v| format!("  {} = '{}'", v.to_uppercase(), v)).collect::<Vec<_>>().join(",\n");
                        enums.push(format!("export type {} = \n{};", enum_name, variants));
                    }
                }
            }
        }

        // Also check relations for enum-like fields (junction tables might have enum fields)
        for conn in &graph.connections {
            if conn.multiplicity == RelationType::ManyToMany {
                // Junction tables are auto-generated, skip for now
            }
        }

        if enums.is_empty() {
            return "// No enums defined\n".to_string();
        }

        format!("export namespace $Enums {{\n{}\n}}\n", enums.join("\n\n"))
    }

    fn generate_types(&self) -> String {
        r#"// Type-level machinery for Valkyrin Client

// _XOR: Associative left-fold for mutually exclusive inputs
export type _XOR<T, U> = T extends U ? never : U extends T ? never : T | U;

// _UnwrapPayloadResult: Extract scalars + composites from Payload
export type _UnwrapPayloadResult<P> = P extends { scalars: infer S; composites: infer C }
  ? S & C
  : never;

// _ApplyOmit: Key remapping with never elimination
export type _ApplyOmit<T, O> = Omit<T, keyof O> & { [K in keyof O as O[K] extends true ? K : never]?: never };

// _DefaultSelection: Dispatches on select/include/omit
export type _DefaultSelection<P, Args, GlobalOmit> =
  Args extends { select: infer S } ? _ApplyOmit<_UnwrapPayloadResult<P>, S> :
  Args extends { include: infer I } ? _ApplyOmit<_UnwrapPayloadResult<P>, I> :
  Args extends { omit: infer O } ? _ApplyOmit<_UnwrapPayloadResult<P>, O> :
  _UnwrapPayloadResult<P>;

// _GetFindResult: Operation dispatcher
export type _GetFindResult<P, Args, Op, GlobalOmit> =
  Op extends 'findUnique' ? _DefaultSelection<P, Args, GlobalOmit> | null :
  Op extends 'findMany' ? _DefaultSelection<P, Args, GlobalOmit>[] :
  Op extends 'findFirst' ? _DefaultSelection<P, Args, GlobalOmit> | null :
  Op extends 'create' ? _DefaultSelection<P, Args, GlobalOmit> :
  Op extends 'update' ? _DefaultSelection<P, Args, GlobalOmit> :
  Op extends 'upsert' ? _DefaultSelection<P, Args, GlobalOmit> :
  Op extends 'delete' ? _DefaultSelection<P, Args, GlobalOmit> :
  never;

// _GetPayloadResult: Extension seam
export type _GetPayloadResult<Base, R> = Omit<Base, _ExtensionKeys<R>> & _ExtensionObject<R>;

export type _ExtensionKeys<R> = R extends { result: infer Res } ? keyof Res : never;
export type _ExtensionObject<R> = R extends { result: infer Res } ? Res : {};

// User-extensible interface
export interface ValkyrinExtensions {
  result?: Record<string, any>;
  query?: Record<string, any>;
}
"#.to_string()
    }

    fn generate_operations(&self, graph: &EntityGraph) -> String {
        let mut output = String::new();
        
        output.push_str("// Operations types for Valkyrin Client\n");
        output.push_str("// Generated from schema.vdb.json\n\n");
        
        // Import the extensions and base types
        output.push_str("import type { ValkyrinExtensions, _GetPayloadResult, _GetFindResult } from './types';\n");
        output.push_str("import type { $Enums } from './enums';\n\n");
        
        // For each entity, generate all operation types
        for entity in &graph.entities {
            if entity.fields.is_empty() {
                continue;
            }
            
            let entity_name = &entity.name;
            let _safe_name = self.sanitize_name(entity_name);
            
            // Build relation map for this entity
            let relations = self.build_relation_map(graph, entity);
            
            // Partition fields
            let (scalars, objects, composites) = self.partition_fields(entity, &relations, graph);
            
            // Generate Payload type
            output.push_str(&self.generate_payload_type(entity_name, &scalars, &objects, &composites, graph));
            output.push_str("\n\n");
            
            // Generate Select type
            output.push_str(&self.generate_select_type(entity_name, &scalars, &objects, &composites, graph));
            output.push_str("\n\n");
            
            // Generate Include type
            output.push_str(&self.generate_include_type(entity_name, &objects, graph));
            output.push_str("\n\n");
            
            // Generate Omit type
            output.push_str(&self.generate_omit_type(entity_name, &scalars, &composites));
            output.push_str("\n\n");
            
            // Generate Where type
            output.push_str(&self.generate_where_type(entity_name, &scalars, &objects, graph));
            output.push_str("\n\n");
            
            // Generate OrderBy type
            output.push_str(&self.generate_orderby_type(entity_name, &scalars));
            output.push_str("\n\n");
            
            // Generate Args types
            output.push_str(&self.generate_find_unique_args(entity_name, &scalars));
            output.push_str("\n\n");
            
            output.push_str(&self.generate_find_many_args(entity_name, &scalars, &objects, graph));
            output.push_str("\n\n");
            
            output.push_str(&self.generate_create_args(entity_name, &scalars, &objects, &composites, graph));
            output.push_str("\n\n");
            
            output.push_str(&self.generate_update_args(entity_name, &scalars, &objects, &composites, graph));
            output.push_str("\n\n");
            
            output.push_str(&self.generate_delete_args(entity_name, &scalars));
            output.push_str("\n\n");
            
            output.push_str(&self.generate_upsert_args(entity_name, &scalars, &objects, &composites, graph));
            output.push_str("\n\n");
            
            // Generate additional input types
            output.push_str(&self.generate_where_unique_input(entity_name, &scalars));
            output.push_str("\n\n");
            
            output.push_str(&self.generate_create_input(entity_name, &scalars, &objects, &composites, graph));
            output.push_str("\n\n");
            
            output.push_str(&self.generate_update_input(entity_name, &scalars, &objects, &composites, graph));
            output.push_str("\n\n");
            
            output.push_str(&self.generate_aggregate_inputs(entity_name, &scalars));
            output.push_str("\n\n");
            
            // Generate Aggregate args
            output.push_str(&self.generate_aggregate_args(entity_name, &scalars));
            output.push_str("\n\n");
            
            // Generate GroupBy args
            output.push_str(&self.generate_group_by_args(entity_name, &scalars));
            output.push_str("\n\n");
        }
        
        output
    }

    fn generate_client(&self, graph: &EntityGraph) -> String {
        let mut output = String::new();
        
        output.push_str(r#"// Valkyrin Client Runtime
// Generated from schema.vdb.json

import type { Pool, PoolClient, QueryResult } from 'pg';
import type { 
  ValkyrinExtensions, 
  _GetPayloadResult, 
  _GetFindResult,
  _DefaultSelection,
  _UnwrapPayloadResult
} from './types';
import type { 
  UserFindUniqueArgs, UserFindManyArgs, UserCreateArgs, UserUpdateArgs, 
  UserDeleteArgs, UserUpsertArgs, UserAggregateArgs, UserGroupByArgs,
  UserWhereInput, UserWhereUniqueInput, UserOrderByInput,
  UserCreateInput, UserUpdateInput, UserSelect, UserInclude, UserOmit,
  UserPayload,
  PostFindUniqueArgs, PostFindManyArgs, PostCreateArgs, PostUpdateArgs,
  PostDeleteArgs, PostUpsertArgs, PostAggregateArgs, PostGroupByArgs,
  PostWhereInput, PostWhereUniqueInput, PostOrderByInput,
  PostCreateInput, PostUpdateInput, PostSelect, PostInclude, PostOmit,
  PostPayload
} from './operations';

"#);

        // Generate the core client runtime
        output.push_str(&self.generate_client_runtime());
        
        // Generate per-entity delegates
        for entity in &graph.entities {
            if entity.fields.is_empty() {
                continue;
            }
            output.push_str(&self.generate_entity_delegate(entity, graph));
        }
        
        // Generate the main ValkyrinClient class
        output.push_str(&self.generate_valkyrin_client_class(graph));
        
        output
    }

    fn generate_index(&self, _graph: &EntityGraph) -> String {
        r#"export * from './types';
export * from './enums';
export * from './operations';
export { ValkyrinClient } from './client';
"#.to_string()
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
    }
}
