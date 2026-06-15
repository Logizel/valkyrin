// valkyrin-core/src/codegen.rs
use crate::ir::{DataType, Entity, Field};
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
            DataType::Decimal { .. } => "decimal.Decimal", // Use shopspring/decimal
            DataType::Boolean => "bool",
            DataType::DateTime => "time.Time",
            DataType::Json => "datatypes.JSON",
            DataType::Uuid => "uuid.UUID",
            DataType::Enum(_) => "string", // Go enums stored as strings, constants generated separately
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

        // Generate enum constants for each enum field
        let enum_fields: Vec<&Field> = entity
            .fields
            .iter()
            .filter(|f| matches!(f.data_type, DataType::Enum(_)))
            .collect();

        if !enum_fields.is_empty() {
            for field in &enum_fields {
                if let DataType::Enum(values) = &field.data_type {
                    let const_name = format!("{}Status", capitalize_first(&field.name));
                    output.push_str(&format!("type {} string\n\n", const_name));
                    output.push_str("const (\n");
                    for val in values {
                        let const_val = format!("{}{}", const_name, capitalize_first(&val));
                        output.push_str(&format!("\t{} {} = \"{}\"\n", const_val, const_name, val));
                    }
                    output.push_str(")\n\n");
                }
            }
        }

        output.push_str(&format!("type {} struct {{\n", entity.name));

        for field in &entity.fields {
            let go_type = self.map_data_type(&field.data_type, field.constraints.is_nullable);

            let mut gorm_tags = vec![format!("column:{}", field.name)];
            if field.constraints.is_primary_key {
                gorm_tags.push("primaryKey".to_string());
            }
            if field.constraints.is_unique {
                gorm_tags.push("unique".to_string());
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
            DataType::Enum(_) => "str",
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
            .filter(|f| matches!(f.data_type, DataType::Enum(_)))
            .collect();

        for field in &enum_fields {
            if let DataType::Enum(values) = &field.data_type {
                let enum_name = format!("{}Enum", capitalize_first(&field.name));
                output.push_str(&format!("class {}(str, Enum):\n", enum_name));
                for val in values {
                    let const_name = val.to_uppercase();
                    output.push_str(&format!("    {} = \"{}\"\n", const_name, val));
                }
                output.push_str("\n");
            }
        }

        output.push_str(&format!("class {}(SQLModel, table=True):\n", entity.name));

        for field in &entity.fields {
            let py_type = if let DataType::Enum(_) = &field.data_type {
                let enum_name = format!("{}Enum", capitalize_first(&field.name));
                if field.constraints.is_nullable {
                    format!("Optional[{}]", enum_name)
                } else {
                    enum_name
                }
            } else {
                self.map_data_type(&field.data_type, field.constraints.is_nullable)
            };

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
            DataType::Enum(_) => "string",
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
            .filter(|f| matches!(f.data_type, DataType::Enum(_)))
            .collect();

        if !enum_fields.is_empty() {
            for field in &enum_fields {
                if let DataType::Enum(values) = &field.data_type {
                    let const_name = format!("{}Status", capitalize_first(&field.name));
                    output.push_str(&format!("type {} string\n\n", const_name));
                    output.push_str("const (\n");
                    for val in values {
                        let const_val = format!("{}{}", const_name, capitalize_first(&val));
                        output.push_str(&format!("\t{} {} = \"{}\"\n", const_val, const_name, val));
                    }
                    output.push_str(")\n\n");
                }
            }
        }

        output.push_str(&format!("type {} struct {{\n", entity.name));
        output.push_str("\tent.Schema\n");
        output.push_str("}\n\n");

        output.push_str(&format!("func ({}) Fields() []ent.Field {{\n", entity.name));
        output.push_str("\treturn []ent.Field{\n");

        for field in &entity.fields {
            let _field_type = self.map_data_type(&field.data_type, field.constraints.is_nullable);
            let _field_name = capitalize_first(&field.name);

            let ent_field_type = match field.data_type {
                DataType::Enum(_) => "String",
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
            if field.constraints.is_unique {
                output.push_str("\t\t\tUnique().\n");
            }
            if field.constraints.is_primary_key {
                output.push_str("\t\t\tDefault(uuid.New).\n");
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
            DataType::Enum(values) => {
                let enum_vals = values.iter().map(|v| format!("'{}'", v)).collect::<Vec<_>>().join(", ");
                format!("Enum({})", enum_vals)
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

        for field in &entity.fields {
            let col_type = self.map_data_type(&field.data_type, field.constraints.is_nullable);

            let mut constraints = String::new();
            if field.constraints.is_primary_key {
                constraints.push_str(", primary_key=True");
            }
            if field.constraints.is_unique {
                constraints.push_str(", unique=True");
            }

            output.push_str(&format!(
                "    {} = Column({}{}, default=None)\n",
                field.name, col_type, constraints
            ));
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
            DataType::Enum(_) => "String".to_string(), // Placeholder, actual type determined in generate_model
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
            .filter(|f| matches!(f.data_type, DataType::Enum(_)))
            .collect();

        for field in &enum_fields {
            if let DataType::Enum(values) = &field.data_type {
                let enum_name = format!("{}Enum", capitalize_first(&field.name));
                output.push_str(&format!("#[derive(Debug, Clone, Copy, PartialEq, Eq, diesel::deserialize::FromSqlRow, diesel::serialize::ToSql)]\n"));
                output.push_str(&format!("#[diesel(sql_type = Text)]\n"));
                output.push_str(&format!("pub enum {} {{\n", enum_name));
                for val in values {
                    let variant = capitalize_first(&val);
                    output.push_str(&format!("    {},\n", variant));
                }
                output.push_str("}\n\n");

                output.push_str(&format!("impl diesel::serialize::ToSql<Text, diesel::pg::Pg> for {} {{\n", enum_name));
                output.push_str("    fn to_sql<'b>(&'b self, out: &mut diesel::serialize::Output<'b, '_, diesel::pg::Pg>) -> diesel::serialize::Result {\n");
                output.push_str("        let s = match self {\n");
                for val in values {
                    let variant = capitalize_first(&val);
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
                    let variant = capitalize_first(&val);
                    output.push_str(&format!("            \"{}\" => Ok({}::{}),\n", val, enum_name, variant));
                }
                output.push_str("            _ => Err(format!(\"Invalid variant for {}: {}\", stringify!({}), s).into()),\n");
                output.push_str("        }\n");
                output.push_str("    }\n");
                output.push_str("}\n\n");
            }
        }

        output.push_str(&format!("#[derive(Queryable, Insertable, Selectable, Serialize, Deserialize, Debug, Clone)]\n"));
        output.push_str(&format!("#[diesel(table_name = {})]\n", entity.name.to_lowercase()));
        output.push_str(&format!("pub struct {} {{\n", entity.name));

        for field in &entity.fields {
            let rust_type = match &field.data_type {
                DataType::Enum(_) => format!("{}Enum", capitalize_first(&field.name)),
                _ => self.map_data_type(&field.data_type, field.constraints.is_nullable),
            };

            let final_type = if field.constraints.is_nullable {
                format!("Option<{}>", rust_type)
            } else {
                rust_type
            };

            output.push_str(&format!("    pub {}: {},\n", field.name, final_type));
        }

        output.push_str("}\n");
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
            DataType::Enum(_) => "String".to_string(), // Placeholder, actual type determined in generate_model
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
            .filter(|f| matches!(f.data_type, DataType::Enum(_)))
            .collect();

        for field in &enum_fields {
            if let DataType::Enum(values) = &field.data_type {
                let enum_name = format!("{}Enum", capitalize_first(&field.name));
                output.push_str(&format!("#[derive(Debug, Clone, PartialEq, EnumIter, DeriveActiveEnum, Serialize, Deserialize)]\n"));
                output.push_str(&format!("#[sea_orm(rs_type = \"String\", db_type = \"Enum\", enum_name = \"{}\")]\n", field.name));
                output.push_str(&format!("pub enum {} {{\n", enum_name));
                for val in values {
                    let variant = capitalize_first(&val);
                    output.push_str(&format!("    #[sea_orm(string_value = \"{}\")]\n", val));
                    output.push_str(&format!("    {},\n", variant));
                }
                output.push_str("}\n\n");
            }
        }

        output.push_str(&format!("#[derive(Clone, Debug, PartialEq, DeriveModel, DeriveActiveModel, Serialize, Deserialize)]\n"));
        output.push_str(&format!("#[sea_orm(table_name = \"{}\")]\n", entity.name.to_lowercase()));
        output.push_str(&format!("pub struct Model {{\n"));

        for field in &entity.fields {
            let sea_type = if let DataType::Enum(_) = &field.data_type {
                let enum_name = format!("{}Enum", capitalize_first(&field.name));
                if field.constraints.is_nullable {
                    format!("Option<{}>", enum_name)
                } else {
                    enum_name
                }
            } else {
                self.map_data_type(&field.data_type, field.constraints.is_nullable)
            };

            let mut attributes = String::new();
            if field.constraints.is_primary_key {
                attributes.push_str("primary_key");
            } else if field.constraints.is_unique {
                attributes.push_str("unique");
            }

            if !attributes.is_empty() {
                output.push_str(&format!("    #[sea_orm({})]\n", attributes));
            }

            output.push_str(&format!("    pub {}: {},\n", field.name, sea_type));
        }

        output.push_str("}\n");
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
            DataType::Enum(values) => {
                let enum_vals = values.iter().map(|v| format!("'{}'", v)).collect::<Vec<_>>().join(", ");
                format!("DataTypes.ENUM({})", enum_vals)
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

        for field in &entity.fields {
            let base_type = self.map_data_type(&field.data_type, field.constraints.is_nullable);

            let mut field_config = format!("type: {}", base_type);
            if field.constraints.is_primary_key {
                field_config.push_str(", primaryKey: true");
            }
            if field.constraints.is_unique {
                field_config.push_str(", unique: true");
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
            DataType::Enum(values) => {
                let enum_vals = values.iter().map(|v| format!("'{}'", v)).collect::<Vec<_>>().join(", ");
                format!("enum({})", enum_vals)
            },
        }
    }

    fn file_extension(&self) -> &'static str {
        "ts"
    }

    fn generate_model(&self, entity: &Entity) -> String {
        let mut output = String::new();

        output.push_str("import { Entity, PrimaryGeneratedColumn, Column } from 'typeorm';\n\n");

        output.push_str(&format!("@Entity('{}')\n", entity.name.to_lowercase()));
        output.push_str(&format!("export class {} {{\n", entity.name));

        for field in &entity.fields {
            if field.constraints.is_primary_key {
                output.push_str("  @PrimaryGeneratedColumn('uuid')\n");
                output.push_str(&format!("  {}: string;\n\n", field.name));
            } else {
                let col_type = self.map_data_type(&field.data_type, field.constraints.is_nullable);
                let mut col_options = String::new();

                if field.constraints.is_unique {
                    col_options.push_str("unique: true, ");
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
                    DataType::Enum(_) => format!("{}Enum", capitalize_first(&field.name)),
                    _ => "string".to_string(),
                };

                output.push_str(&format!("  {}: {};\n\n", field.name, ts_type));
            }
        }

        // Add enum type definitions at the end
        let enum_fields: Vec<&Field> = entity
            .fields
            .iter()
            .filter(|f| matches!(f.data_type, DataType::Enum(_)))
            .collect();

        if !enum_fields.is_empty() {
            for field in &enum_fields {
                if let DataType::Enum(values) = &field.data_type {
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
            DataType::Enum(values) => {
                let enum_name = capitalize_first(
                    values.first().map(|v| v.as_str()).unwrap_or("Status")
                );
                format!("{}", enum_name)
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
            .filter(|f| matches!(f.data_type, DataType::Enum(_)))
            .collect();

        if !enum_fields.is_empty() {
            for field in &enum_fields {
                if let DataType::Enum(values) = &field.data_type {
                    let enum_name = format!("{}", capitalize_first(&field.name));
                    output.push_str(&format!("enum {} {{\n", enum_name));
                    for val in values {
                        let variant = val.to_uppercase();
                        output.push_str(&format!("  {}\n", variant));
                    }
                    output.push_str("}\n\n");
                }
            }
        }

        output.push_str(&format!("model {} {{\n", entity.name));

        for field in &entity.fields {
            let prisma_type = if field.constraints.is_primary_key && matches!(field.data_type, DataType::Uuid) {
                "String @id @default(uuid())".to_string()
            } else if let DataType::Enum(_) = &field.data_type {
                let enum_name = format!("{}", capitalize_first(&field.name));
                if field.constraints.is_nullable {
                    format!("{}?", enum_name)
                } else {
                    enum_name
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

            if field.constraints.is_primary_key && !matches!(field.data_type, DataType::Uuid) {
                output.push_str(" @id");
            }
            if field.constraints.is_unique && !field.constraints.is_primary_key {
                output.push_str(" @unique");
            }

            output.push_str("\n");
        }

        output.push_str("}\n");
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
            DataType::Enum(values) => {
                let enum_vals = values.iter().map(|v| format!("'{}'", v)).collect::<Vec<_>>().join(", ");
                format!("enum({})", enum_vals)
            },
        }
    }

    fn file_extension(&self) -> &'static str {
        "ts"
    }

    fn generate_model(&self, entity: &Entity) -> String {
        let mut output = String::new();

        output.push_str("import { Entity, PrimaryGeneratedColumn, Column } from 'typeorm';\n\n");

        output.push_str(&format!("@Entity('{}')\n", entity.name.to_lowercase()));
        output.push_str(&format!("export class {} {{\n", entity.name));

        for field in &entity.fields {
            if field.constraints.is_primary_key {
                output.push_str("  @PrimaryGeneratedColumn('uuid')\n");
                output.push_str(&format!("  {}: string;\n\n", field.name));
            } else {
                let col_type = self.map_data_type(&field.data_type, field.constraints.is_nullable);
                let mut col_options = String::new();

                if field.constraints.is_unique {
                    col_options.push_str("unique: true, ");
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

                let ts_type = match field.data_type {
                    DataType::Boolean => "boolean".to_string(),
                    DataType::Integer(_) => "number".to_string(),
                    DataType::Float | DataType::Decimal { .. } => "number".to_string(),
                    DataType::Enum(_) => format!("{}Enum", capitalize_first(&field.name)),
                    _ => "string".to_string(),
                };

                output.push_str(&format!("  {}: {};\n\n", field.name, ts_type));
            }
        }

        let enum_fields: Vec<&Field> = entity
            .fields
            .iter()
            .filter(|f| matches!(f.data_type, DataType::Enum(_)))
            .collect();

        if !enum_fields.is_empty() {
            for field in &enum_fields {
                if let DataType::Enum(values) = &field.data_type {
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
