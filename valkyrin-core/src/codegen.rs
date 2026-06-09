// valkyrin-core/src/codegen.rs
use crate::ir::{DataType, Entity};

/// The universal contract for code generation.
pub trait LanguageDriver {
    /// Translates a universal Valkyrin type into the language-specific type.
    fn map_data_type(&self, data_type: &DataType, is_nullable: bool) -> String;

    /// Generates the necessary file imports (e.g., `import "time"` for Go).
    fn generate_imports(&self, entity: &Entity) -> String;

    /// Compiles a single entity into a full struct/class definition.
    fn generate_model(&self, entity: &Entity) -> String;
}

pub struct GoDriver;

impl LanguageDriver for GoDriver {
    fn map_data_type(&self, data_type: &DataType, is_nullable: bool) -> String {
        let base_type = match data_type {
            DataType::String { .. } | DataType::Text => "string",
            DataType::Integer(_) => "int",
            DataType::Float => "float64",
            DataType::Boolean => "bool",
            DataType::DateTime => "time.Time",
            DataType::Json => "datatypes.JSON",
            DataType::Uuid => "uuid.UUID",
        };

        // If a column is nullable, Go requires a memory pointer to handle the nil state
        if is_nullable {
            format!("*{}", base_type)
        } else {
            base_type.to_string()
        }
    }

    fn generate_imports(&self, entity: &Entity) -> String {
        let mut imports = vec!["\"time\""];

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

        format!("import (\n\t{}\n)", imports.join("\n\t"))
    }

    fn generate_model(&self, entity: &Entity) -> String {
        let mut model = format!("type {} struct {{\n", entity.name);

        for field in &entity.fields {
            let go_type = self.map_data_type(&field.data_type, field.constraints.is_nullable);

            // Build the GORM struct tags deterministically
            let mut gorm_tags = vec![format!("column:{}", field.name)];
            if field.constraints.is_primary_key {
                gorm_tags.push("primaryKey".to_string());
            }
            if field.constraints.is_unique {
                gorm_tags.push("unique".to_string());
            }

            // Go struct fields must be capitalized to be public/exported
            let exported_name = capitalize_first(&field.name);
            model.push_str(&format!(
                "\t{} {} `gorm:\"{}\" json:\"{}\"`\n",
                exported_name,
                go_type,
                gorm_tags.join(";"),
                field.name
            ));
        }

        model.push_str("}\n");
        model
    }
}

fn capitalize_first(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        None => String::new(),
        Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
    }
}

pub struct PythonDriver;

impl LanguageDriver for PythonDriver {
    fn map_data_type(&self, data_type: &DataType, is_nullable: bool) -> String {
        let base_type = match data_type {
            DataType::String { .. } | DataType::Text => "str",
            DataType::Integer(_) => "int",
            DataType::Float => "float",
            DataType::Boolean => "bool",
            DataType::DateTime => "datetime",
            DataType::Json => "dict",
            DataType::Uuid => "UUID",
        };

        if is_nullable {
            format!("Optional[{}]", base_type)
        } else {
            base_type.to_string()
        }
    }

    fn generate_imports(&self, _entity: &Entity) -> String {
        "from typing import Optional\nfrom datetime import datetime\nfrom sqlmodel import SQLModel, Field".to_string()
    }

    fn generate_model(&self, entity: &Entity) -> String {
        let mut model = format!("class {}(SQLModel, table=True):\n", entity.name);

        for field in &entity.fields {
            let py_type = self.map_data_type(&field.data_type, field.constraints.is_nullable);

            let primary_key_flag = if field.constraints.is_primary_key {
                "primary_key=True"
            } else {
                ""
            };

            if primary_key_flag.is_empty() {
                model.push_str(&format!("    {}: {}\n", field.name, py_type));
            } else {
                model.push_str(&format!(
                    "    {}: {} = Field(default=None, {})\n",
                    field.name, py_type, primary_key_flag
                ));
            }
        }

        model
    }
}

pub enum TargetLanguage {
    Go,
    Python,
    Rust,
    TypeScript,
}

pub fn get_driver(language: TargetLanguage) -> Box<dyn LanguageDriver> {
    match language {
        TargetLanguage::Go => Box::new(GoDriver),
        TargetLanguage::Python => Box::new(PythonDriver),
        // Future driver stubs return unimplemented!() for now
        TargetLanguage::Rust => unimplemented!("Rust driver pending"),
        TargetLanguage::TypeScript => unimplemented!("TS driver pending"),
    }
}
