// valkyrin-core/src/validate.rs
use crate::error::{ValkyrinError, ValkyrinResult};
use crate::ir::{Entity, DataType};
use colored::*;
use std::collections::HashSet;

/// Validation rules for schema
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ValidationRule {
    /// Primary key columns must not be nullable
    NoNullablePk,
    /// Foreign key columns should be indexed
    FkIndexed,
    /// Enum types must have values
    EnumHasValues,
    /// No duplicate entity names (case-insensitive)
    NoDuplicateEntities,
    /// No reserved words used as identifiers
    NoReservedNames,
    /// Tables must have at least one column
    TableHasColumns,
}

impl ValidationRule {
    /// Get the rule name for error reporting
    pub fn name(&self) -> &'static str {
        match self {
            ValidationRule::NoNullablePk => "no_nullable_pk",
            ValidationRule::FkIndexed => "fk_indexed",
            ValidationRule::EnumHasValues => "enum_has_values",
            ValidationRule::NoDuplicateEntities => "no_duplicate_entities",
            ValidationRule::NoReservedNames => "no_reserved_names",
            ValidationRule::TableHasColumns => "table_has_columns",
        }
    }

    /// Get the error code for this rule
    pub fn error_code(&self) -> &'static str {
        match self {
            ValidationRule::NoNullablePk => "VAL-008",
            ValidationRule::FkIndexed => "VAL-008",
            ValidationRule::EnumHasValues => "VAL-008",
            ValidationRule::NoDuplicateEntities => "VAL-008",
            ValidationRule::NoReservedNames => "VAL-008",
            ValidationRule::TableHasColumns => "VAL-008",
        }
    }

    /// Check a single entity against this rule
    pub fn check_entity(&self, entity: &Entity, all_entities: &[Entity]) -> Option<String> {
        match self {
            ValidationRule::NoNullablePk => {
                for field in &entity.fields {
                    if field.constraints.is_primary_key && field.constraints.is_nullable {
                        return Some(format!(
                            "Entity '{}': primary key field '{}' cannot be nullable",
                            entity.name, field.name
                        ));
                    }
                }
                None
            }
            ValidationRule::FkIndexed => {
                for field in &entity.fields {
                    // Check if field name looks like a foreign key (ends with _id)
                    if field.name.ends_with("_id") && !field.constraints.is_indexed && !field.constraints.is_primary_key {
                        return Some(format!(
                            "Entity '{}': foreign key field '{}' should be indexed",
                            entity.name, field.name
                        ));
                    }
                }
                None
            }
            ValidationRule::EnumHasValues => {
                for field in &entity.fields {
                    if let DataType::Enum(values) = &field.data_type {
                        if values.is_empty() {
                            return Some(format!(
                                "Entity '{}': enum field '{}' must have at least one value",
                                entity.name, field.name
                            ));
                        }
                    }
                }
                None
            }
            ValidationRule::NoDuplicateEntities => {
                let mut names = HashSet::new();
                for e in all_entities {
                    let lower = e.name.to_lowercase();
                    if !names.insert(lower.clone()) {
                        return Some(format!(
                            "Duplicate entity name (case-insensitive): '{}'",
                            lower
                        ));
                    }
                }
                None
            }
            ValidationRule::NoReservedNames => {
                let reserved = get_reserved_words();
                if reserved.contains(&entity.name.to_lowercase().as_str()) {
                    return Some(format!(
                        "Entity '{}' uses a reserved word",
                        entity.name
                    ));
                }
                for field in &entity.fields {
                    if reserved.contains(&field.name.to_lowercase().as_str()) {
                        return Some(format!(
                            "Entity '{}': field '{}' uses a reserved word",
                            entity.name, field.name
                        ));
                    }
                }
                None
            }
            ValidationRule::TableHasColumns => {
                if entity.fields.is_empty() {
                    return Some(format!(
                        "Entity '{}' has no columns",
                        entity.name
                    ));
                }
                None
            }
        }
    }
}

/// Get reserved SQL/language keywords
fn get_reserved_words() -> HashSet<&'static str> {
    let mut set = HashSet::new();
    // SQL keywords
    set.insert("select");
    set.insert("from");
    set.insert("where");
    set.insert("insert");
    set.insert("update");
    set.insert("delete");
    set.insert("create");
    set.insert("drop");
    set.insert("alter");
    set.insert("table");
    set.insert("index");
    set.insert("primary");
    set.insert("key");
    set.insert("foreign");
    set.insert("references");
    set.insert("unique");
    set.insert("null");
    set.insert("not");
    set.insert("and");
    set.insert("or");
    set.insert("order");
    set.insert("by");
    set.insert("group");
    set.insert("having");
    set.insert("join");
    set.insert("inner");
    set.insert("left");
    set.insert("right");
    set.insert("full");
    set.insert("outer");
    set.insert("on");
    set.insert("as");
    set.insert("distinct");
    set.insert("limit");
    set.insert("offset");
    set.insert("union");
    set.insert("values");
    set.insert("default");
    set.insert("check");
    set.insert("constraint");
    // Go
    set.insert("break");
    set.insert("case");
    set.insert("chan");
    set.insert("const");
    set.insert("continue");
    set.insert("default");
    set.insert("defer");
    set.insert("else");
    set.insert("fallthrough");
    set.insert("for");
    set.insert("func");
    set.insert("go");
    set.insert("goto");
    set.insert("if");
    set.insert("import");
    set.insert("interface");
    set.insert("map");
    set.insert("package");
    set.insert("range");
    set.insert("return");
    set.insert("select");
    set.insert("struct");
    set.insert("switch");
    set.insert("type");
    set.insert("var");
    // Python
    set.insert("and");
    set.insert("as");
    set.insert("assert");
    set.insert("async");
    set.insert("await");
    set.insert("break");
    set.insert("class");
    set.insert("continue");
    set.insert("def");
    set.insert("del");
    set.insert("elif");
    set.insert("else");
    set.insert("except");
    set.insert("finally");
    set.insert("for");
    set.insert("from");
    set.insert("global");
    set.insert("if");
    set.insert("import");
    set.insert("in");
    set.insert("is");
    set.insert("lambda");
    set.insert("nonlocal");
    set.insert("not");
    set.insert("or");
    set.insert("pass");
    set.insert("raise");
    set.insert("return");
    set.insert("try");
    set.insert("while");
    set.insert("with");
    set.insert("yield");
    // Rust
    set.insert("as");
    set.insert("break");
    set.insert("const");
    set.insert("continue");
    set.insert("crate");
    set.insert("else");
    set.insert("enum");
    set.insert("extern");
    set.insert("false");
    set.insert("fn");
    set.insert("for");
    set.insert("if");
    set.insert("impl");
    set.insert("in");
    set.insert("let");
    set.insert("loop");
    set.insert("match");
    set.insert("mod");
    set.insert("move");
    set.insert("mut");
    set.insert("pub");
    set.insert("ref");
    set.insert("return");
    set.insert("self");
    set.insert("static");
    set.insert("struct");
    set.insert("super");
    set.insert("trait");
    set.insert("true");
    set.insert("type");
    set.insert("unsafe");
    set.insert("use");
    set.insert("where");
    set.insert("while");
    // TypeScript/JavaScript
    set.insert("break");
    set.insert("case");
    set.insert("catch");
    set.insert("class");
    set.insert("const");
    set.insert("continue");
    set.insert("debugger");
    set.insert("default");
    set.insert("delete");
    set.insert("do");
    set.insert("else");
    set.insert("enum");
    set.insert("export");
    set.insert("extends");
    set.insert("false");
    set.insert("finally");
    set.insert("for");
    set.insert("function");
    set.insert("if");
    set.insert("import");
    set.insert("in");
    set.insert("instanceof");
    set.insert("new");
    set.insert("null");
    set.insert("return");
    set.insert("super");
    set.insert("switch");
    set.insert("this");
    set.insert("throw");
    set.insert("true");
    set.insert("try");
    set.insert("typeof");
    set.insert("var");
    set.insert("void");
    set.insert("while");
    set.insert("with");
    set
}

/// Validate the entire schema
pub async fn validate_schema(strict: bool) -> ValkyrinResult<()> {
    use crate::canvas::CanvasPayload;
    use std::fs;

    let file_contents = fs::read_to_string("schema.vdb.json")
        .map_err(|e| ValkyrinError::Io(e.to_string()))?;

    let payload: CanvasPayload = serde_json::from_str(&file_contents)
        .map_err(|e| ValkyrinError::Parse(e.to_string()))?;

    let ir_graph = payload.to_ir();

    if ir_graph.entities.is_empty() {
        return Err(ValkyrinError::Validation(
            "Canvas is empty (no tables defined)".to_string()
        ));
    }

    let mut errors = Vec::new();
    let mut warnings = Vec::new();

    let rules = [
        ValidationRule::NoNullablePk,
        ValidationRule::FkIndexed,
        ValidationRule::EnumHasValues,
        ValidationRule::NoDuplicateEntities,
        ValidationRule::NoReservedNames,
        ValidationRule::TableHasColumns,
    ];

    for entity in &ir_graph.entities {
        for rule in &rules {
            if let Some(msg) = rule.check_entity(entity, &ir_graph.entities) {
                // Primary key nullable is an error, others are warnings
                match rule {
                    ValidationRule::NoNullablePk => {
                        errors.push(format!("{}: {}", rule.error_code(), msg));
                    }
                    _ => {
                        if strict {
                            errors.push(format!("{}: {}", rule.error_code(), msg));
                        } else {
                            warnings.push(format!("{}: {}", rule.error_code(), msg));
                        }
                    }
                }
            }
        }
    }

    if !warnings.is_empty() {
        println!("{}", "⚠️  Warnings:".yellow().bold());
        for w in &warnings {
            println!("  {}", w.yellow());
        }
    }

    if !errors.is_empty() {
        println!("{}", "❌ Errors:".red().bold());
        for e in &errors {
            println!("  {}", e.red());
        }
        return Err(ValkyrinError::Validation(
            format!("Schema validation failed with {} error(s)", errors.len())
        ));
    }

    println!("{} Schema validation passed!", "=>".green().bold());
    Ok(())
}