// valkyrin-core/src/codegen.rs
use crate::ir::{DataType, Entity};
use anyhow::Result;

/// The universal contract for code generation.
pub trait LanguageDriver {
    /// Translates a universal Valkyrin type into the language-specific type.
    fn map_data_type(&self, data_type: &DataType, is_nullable: bool) -> String;

    /// Compiles a single entity into a complete file with imports and struct/class definition.
    fn generate_model(&self, entity: &Entity) -> String;

    /// Returns the file extension for this language (e.g., "go", "py", "rs").
    fn file_extension(&self) -> &'static str;
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
            DataType::Decimal { .. } => "float64", // Go doesn't have native decimal; use float64 or use a lib
            DataType::Boolean => "bool",
            DataType::DateTime => "time.Time",
            DataType::Json => "datatypes.JSON",
            DataType::Uuid => "uuid.UUID",
            DataType::Enum(_) => "string", // Go enums are typically strings or consts
        };

        // If a column is nullable, Go requires a memory pointer to handle the nil state
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

        // Generate package declaration (Go-specific)
        output.push_str("package models\n\n");

        // Generate imports
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

        output.push_str(&format!("import (\n\t{}\n)\n\n", imports.join("\n\t")));

        // Generate struct definition
        output.push_str(&format!("type {} struct {{\n", entity.name));

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
            DataType::Enum(_) => "str", // Python enums are typically Enum class, but store as str
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

        // Generate imports
        output.push_str("from typing import Optional\nfrom datetime import datetime\nfrom sqlmodel import SQLModel, Field\n\n");

        // Generate class definition
        output.push_str(&format!("class {}(SQLModel, table=True):\n", entity.name));

        for field in &entity.fields {
            let py_type = self.map_data_type(&field.data_type, field.constraints.is_nullable);

            let primary_key_flag = if field.constraints.is_primary_key {
                "primary_key=True"
            } else {
                ""
            };

            if primary_key_flag.is_empty() {
                output.push_str(&format!("    {}: {}\n", field.name, py_type));
            } else {
                output.push_str(&format!(
                    "    {}: {} = Field(default=None, {})\n",
                    field.name, py_type, primary_key_flag
                ));
            }
        }

        output
    }
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
}

pub fn get_driver(backend: TargetBackend) -> Result<Box<dyn LanguageDriver>> {
    match backend {
        TargetBackend::GoGorm => Ok(Box::new(GoGormDriver)),
        TargetBackend::PythonSqlModel => Ok(Box::new(PythonSqlModelDriver)),
        // Remaining drivers return errors with descriptive messages
        TargetBackend::GoEnt => Err(anyhow::anyhow!("GoEnt driver not yet implemented")),
        TargetBackend::PythonSqlAlchemy => Err(anyhow::anyhow!("PythonSqlAlchemy driver not yet implemented")),
        TargetBackend::RustDiesel => Err(anyhow::anyhow!("RustDiesel driver not yet implemented")),
        TargetBackend::RustSeaOrm => Err(anyhow::anyhow!("RustSeaORM driver not yet implemented")),
        TargetBackend::JavaScriptSequelize => Err(anyhow::anyhow!("JavaScriptSequelize driver not yet implemented")),
        TargetBackend::JavaScriptTypeOrm => Err(anyhow::anyhow!("JavaScriptTypeOrm driver not yet implemented")),
        TargetBackend::TypeScriptPrisma => Err(anyhow::anyhow!("TypeScriptPrisma driver not yet implemented")),
        TargetBackend::TypeScriptTypeOrm => Err(anyhow::anyhow!("TypeScriptTypeOrm driver not yet implemented")),
    }
}
