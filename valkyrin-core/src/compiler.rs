// valkyrin-core/src/compiler.rs
use crate::ast::CodeMerger;
use crate::canvas::CanvasPayload;
use crate::codegen::{TargetLanguage, get_driver};
use anyhow::{Context, Result};
use std::fs;

pub fn compile_blueprint() -> Result<()> {
    let file_contents = fs::read_to_string("schema.vdb.json")
        .context("Could not find schema.vdb.json. Have you saved the canvas yet?")?;

    let payload: CanvasPayload = serde_json::from_str(&file_contents)
        .context("Failed to parse schema.vdb.json. The file might be corrupted.")?;

    // Pass 1: Parse IR Scaffolding
    let mut ir_graph = payload.to_ir();

    // Pass 2: The Relational Constraint Injector
    // This evaluates graph edges and injects Foreign Keys where needed
    let connections = ir_graph.connections.clone();
    for conn in connections {
        // Find the name of the Source Table (e.g., "Users")
        let source_table_name = ir_graph
            .entities
            .iter()
            .find(|e| e.id == conn.source_entity_id)
            .map(|e| e.name.clone())
            .unwrap_or_default();

        // Inject the Foreign Key into the Target Table (e.g., "Sessions")
        if let Some(target_table) = ir_graph
            .entities
            .iter_mut()
            .find(|e| e.id == conn.target_entity_id)
        {
            let fk_name = format!("{}_id", source_table_name.to_lowercase());

            // Prevent duplicate injections if the developer already added the FK manually on the canvas
            if !target_table.fields.iter().any(|f| f.name == fk_name) {
                target_table.fields.push(crate::ir::Field {
                    id: format!("fk_{}", conn.source_entity_id),
                    name: fk_name,
                    data_type: crate::ir::DataType::String { max_length: None }, // Simplified FK type mapping
                    constraints: crate::ir::Constraints {
                        is_primary_key: false,
                        is_unique: false,
                        is_nullable: true,
                    },
                });
            }
        }
    }

    // Initialize Language Drivers
    let driver = get_driver(TargetLanguage::Go);
    let merger = CodeMerger::new_go();

    let output_dir = "models";
    fs::create_dir_all(output_dir)?;

    // Pass 3: Run Generation & AST Merge Loop
    for entity in ir_graph.entities {
        let safe_name = entity.name.to_lowercase().replace(" ", "_");
        let filename = format!("{}/{}.go", output_dir, safe_name);

        let imports = driver.generate_imports(&entity);
        let struct_def = driver.generate_model(&entity);
        let combined_schema_code = format!("package models\n\n{}\n\n{}", imports, struct_def);

        let custom_code = merger.extract_custom_zones(&filename);

        merger.stitch_and_write(&filename, combined_schema_code, custom_code)?;
    }

    Ok(())
}
