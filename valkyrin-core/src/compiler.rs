// valkyrin-core/src/compiler.rs
use crate::ast::CodeMerger;
use crate::canvas::CanvasPayload;
use crate::codegen::{TargetBackend, get_driver};
use crate::config::load_config;
use anyhow::{Context, Result, ensure};
use std::collections::{HashMap, HashSet};
use std::fs;

/// Reserved words by language — prevents generating invalid code
const GO_RESERVED: &[&str] = &[
    "break", "case", "chan", "const", "continue", "default", "defer", "else",
    "fallthrough", "for", "func", "go", "goto", "if", "import", "interface",
    "map", "package", "range", "return", "select", "struct", "switch", "type", "var",
];
const PYTHON_RESERVED: &[&str] = &[
    "and", "as", "assert", "async", "await", "break", "class", "continue", "def",
    "del", "elif", "else", "except", "finally", "for", "from", "global", "if",
    "import", "in", "is", "lambda", "nonlocal", "not", "or", "pass", "raise",
    "return", "try", "while", "with", "yield",
];
const RUST_RESERVED: &[&str] = &[
    "as", "break", "const", "continue", "crate", "else", "enum", "extern", "false",
    "fn", "for", "if", "impl", "in", "let", "loop", "match", "mod", "move", "mut",
    "pub", "ref", "return", "self", "static", "struct", "super", "trait", "true",
    "type", "unsafe", "use", "where", "while",
];
const TS_RESERVED: &[&str] = &[
    "break", "case", "catch", "class", "const", "continue", "debugger", "default",
    "delete", "do", "else", "enum", "export", "extends", "false", "finally", "for",
    "function", "if", "import", "in", "instanceof", "new", "null", "return",
    "super", "switch", "this", "throw", "true", "try", "typeof", "var", "void",
    "while", "with",
    "valkyrinclient", "valkyrin", "delegate", "payload", "extensions",
];

fn get_reserved_words(backend: &TargetBackend) -> &'static [&'static str] {
    match backend {
        TargetBackend::GoGorm | TargetBackend::GoEnt => GO_RESERVED,
        TargetBackend::PythonSqlAlchemy | TargetBackend::PythonSqlModel => PYTHON_RESERVED,
        TargetBackend::RustDiesel | TargetBackend::RustSeaOrm => RUST_RESERVED,
        TargetBackend::TypeScriptPrisma
        | TargetBackend::TypeScriptTypeOrm
        | TargetBackend::JavaScriptSequelize
        | TargetBackend::JavaScriptTypeOrm
        | TargetBackend::TypeScriptValkyrin => TS_RESERVED,
    }
}

/// Sanitize an identifier: ensure it's not a reserved keyword, append `_` if it is
fn sanitize_name(name: &str, backend: &TargetBackend) -> String {
    let reserved = get_reserved_words(backend);
    if reserved.iter().any(|r| *r == name.to_lowercase()) {
        format!("{}_", name)
    } else {
        name.to_string()
    }
}

/// Ensure an identifier is filesystem-safe and not empty
fn safe_filename(name: &str) -> String {
    let sanitized: String = name
        .chars()
        .map(|c| if c.is_alphanumeric() || c == '_' || c == '-' { c } else { '_' })
        .collect();
    if sanitized.is_empty() || sanitized.starts_with(|c: char| c.is_ascii_digit()) {
        format!("table_{}", sanitized)
    } else {
        sanitized.to_lowercase()
    }
}

pub fn compile_blueprint() -> Result<()> {
    let config = load_config()?;

    let file_contents = fs::read_to_string("schema.vdb.json")
        .context("Could not find schema.vdb.json. Have you saved the canvas yet?")?;

    let payload: CanvasPayload =
        serde_json::from_str(&file_contents).context("Failed to parse schema.vdb.json")?;

    ensure!(
        !payload.tables.is_empty(),
        "Canvas is empty (no tables defined). Add tables on the canvas and save before generating."
    );

    let mut ir_graph = payload.to_ir();

    // Phase 6 Edge Case: Detect duplicate entity names
    let mut name_counts: HashMap<String, usize> = HashMap::new();
    for entity in &ir_graph.entities {
        *name_counts.entry(entity.name.to_lowercase()).or_default() += 1;
    }
    let duplicates: Vec<&String> = name_counts
        .iter()
        .filter(|(_, count)| **count > 1)
        .map(|(name, _)| name)
        .collect();
    if !duplicates.is_empty() {
        // Disambiguate: append _1, _2 suffix
        let mut seen: HashMap<String, usize> = HashMap::new();
        for entity in &mut ir_graph.entities {
            let key = entity.name.to_lowercase();
            if *name_counts.get(&key).unwrap_or(&1) > 1 {
                let count = seen.entry(key.clone()).or_insert(0);
                *count += 1;
                if *count > 1 {
                    entity.name = format!("{}_{}", entity.name, count);
                }
            }
        }
    }

    // Phase 6 Edge Case: Warn about tables with no columns
    let empty_tables: Vec<&str> = ir_graph
        .entities
        .iter()
        .filter(|e| e.fields.is_empty())
        .map(|e| e.name.as_str())
        .collect();
    if !empty_tables.is_empty() {
        println!(
            "⚠️  Warning: {} table(s) have no columns and will be skipped: {}",
            empty_tables.len(),
            empty_tables.join(", ")
        );
    }

    // Phase 3.1: Auto-generate junction entities for ManyToMany relations
    let mut junction_entities = Vec::new();
    let connections = ir_graph.connections.clone();
    for conn in &connections {
        if conn.multiplicity == crate::ir::RelationType::ManyToMany {
            // Find source and target entities
            let source_entity = ir_graph.entities.iter().find(|e| e.id == conn.source_entity_id);
            let target_entity = ir_graph.entities.iter().find(|e| e.id == conn.target_entity_id);
            
            if let (Some(source), Some(target)) = (source_entity, target_entity) {
                // Create junction table name: alphabetical join (e.g., user_group for User↔Group)
                let source_name = source.name.to_lowercase();
                let target_name = target.name.to_lowercase();
                let junction_name = if source_name < target_name {
                    format!("{}_{}", source_name, target_name)
                } else {
                    format!("{}_{}", target_name, source_name)
                };
                
                // Check if junction entity already exists
                let junction_exists = ir_graph.entities.iter().any(|e| e.name == junction_name);
                if junction_exists {
                    continue;
                }
                
                // Get PK data types from source and target
                let source_pk_type = source
                    .fields
                    .iter()
                    .find(|f| f.constraints.is_primary_key)
                    .map(|f| f.data_type.clone())
                    .unwrap_or(crate::ir::DataType::Uuid);
                let target_pk_type = target
                    .fields
                    .iter()
                    .find(|f| f.constraints.is_primary_key)
                    .map(|f| f.data_type.clone())
                    .unwrap_or(crate::ir::DataType::Uuid);
                
                // Create junction entity with two FK columns + composite unique index
                let mut junction_fields = Vec::new();
                
                // First FK column
                let source_fk_name = format!("{}_id", source.name.to_lowercase());
                junction_fields.push(crate::ir::Field {
                    id: format!("fk_{}", conn.source_entity_id),
                    name: source_fk_name.clone(),
                    data_type: source_pk_type,
                    constraints: crate::ir::Constraints {
                        is_primary_key: true,
                        primary_key_order: Some(0),
                        is_unique: false,
                        is_nullable: false,
                        is_indexed: true,
                        default_value: None,
                    },
                    is_composite: false,
                });
                
                // Second FK column
                let target_fk_name = format!("{}_id", target.name.to_lowercase());
                junction_fields.push(crate::ir::Field {
                    id: format!("fk_{}", conn.target_entity_id),
                    name: target_fk_name.clone(),
                    data_type: target_pk_type,
                    constraints: crate::ir::Constraints {
                        is_primary_key: true,
                        primary_key_order: Some(1),
                        is_unique: false,
                        is_nullable: false,
                        is_indexed: true,
                        default_value: None,
                    },
                });
                
                // Create the junction entity
                junction_entities.push(crate::ir::Entity {
                    id: format!("junction_{}_{}", conn.source_entity_id, conn.target_entity_id),
                    name: junction_name,
                    fields: junction_fields,
                });
            }
        }
    }
    
    // Add junction entities to IR graph
    ir_graph.entities.extend(junction_entities);

    // Map the yaml string and orm to our TargetBackend enum
    let target_backend = match (
        config.language.to_lowercase().as_str(),
        config.orm.as_ref().map(|s| s.to_lowercase()).as_deref(),
    ) {
        ("python", Some("sqlalchemy")) => TargetBackend::PythonSqlAlchemy,
        ("python", _) => TargetBackend::PythonSqlModel,
        ("rust", Some("diesel")) => TargetBackend::RustDiesel,
        ("rust", _) => TargetBackend::RustSeaOrm,
        ("typescript", Some("prisma")) => TargetBackend::TypeScriptPrisma,
        ("typescript", _) => TargetBackend::TypeScriptTypeOrm,
        ("javascript", Some("typeorm")) => TargetBackend::JavaScriptTypeOrm,
        ("javascript", _) => TargetBackend::JavaScriptSequelize,
        ("go", Some("ent")) => TargetBackend::GoEnt,
        ("go", _) => TargetBackend::GoGorm,
        _ => TargetBackend::GoGorm,
    };

    let driver = get_driver(target_backend)?;
    let output_dir = "models";
    fs::create_dir_all(output_dir)?;

    let mut generated_entity_names = HashSet::new();

    // Pass 2: The Relational Constraint Injector
    let connections = ir_graph.connections.clone();
    for conn in &connections {
        let (source_table_name, source_pk_data_type) = ir_graph
            .entities
            .iter()
            .find(|e| e.id == conn.source_entity_id)
            .map(|e| {
                let pk_data_type = e
                    .fields
                    .iter()
                    .find(|f| f.constraints.is_primary_key)
                    .map(|f| f.data_type.clone())
                    .unwrap_or(crate::ir::DataType::Uuid);
                (e.name.clone(), pk_data_type)
            })
            .unwrap_or((String::new(), crate::ir::DataType::Uuid));

        if let Some(target_table) = ir_graph
            .entities
            .iter_mut()
            .find(|e| e.id == conn.target_entity_id)
        {
            let fk_name = format!("{}_id", source_table_name.to_lowercase());

            if !target_table.fields.iter().any(|f| f.name == fk_name) {
                target_table.fields.push(crate::ir::Field {
                    id: format!("fk_{}", conn.source_entity_id),
                    name: fk_name,
                    data_type: source_pk_data_type,
                    constraints: crate::ir::Constraints {
                        is_primary_key: false,
                        primary_key_order: None,
                        is_unique: false,
                        is_nullable: true,
                        is_indexed: false,
                        default_value: None,
                    },
                });
            }
        }
    }

    // Pass 3: Run Generation & AST Merge Loop
    for entity in &ir_graph.entities {
        // Skip empty tables
        if entity.fields.is_empty() {
            continue;
        }

        // Sanitize entity name (reserved word protection + safe filename)
        let display_name = sanitize_name(&entity.name, &target_backend);
        let safe_name = safe_filename(&display_name);
        generated_entity_names.insert(safe_name.clone());

        let ext = driver.file_extension();
        let filename = format!("{}/{}.{}", output_dir, safe_name, ext);

        let generated_code = driver.generate_model(entity);

        let merger = CodeMerger::new(&target_backend);
        let custom_code = merger.extract_custom_zones(&filename, &target_backend);

        merger.stitch_and_write(&filename, generated_code, custom_code)?;
    }

    // Diff-and-prune: Delete orphaned files
    let managed_extensions = ["go", "py", "rs", "ts", "js", "prisma"];
    if let Ok(entries) = fs::read_dir(output_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file()
                && let Some(ext) = path.extension().and_then(|e| e.to_str())
                    && managed_extensions.contains(&ext)
                        && let Some(stem) = path.file_stem().and_then(|s| s.to_str())
                            && !generated_entity_names.contains(stem) {
                                fs::remove_file(&path)?;
                            }
        }
    }

    Ok(())
}
