// valkyrin-core/src/compiler.rs
use crate::ast::CodeMerger;
use crate::canvas::CanvasPayload;
use crate::codegen::{TargetLanguage, get_driver};
use anyhow::{Context, Result};
use std::fs;

pub fn compile_blueprint() -> Result<()> {
    // 1. Read the blueprint from disk
    let file_contents = fs::read_to_string("schema.vdb.json")
        .context("Could not find schema.vdb.json. Have you saved the canvas yet?")?;

    let payload: CanvasPayload = serde_json::from_str(&file_contents)
        .context("Failed to parse schema.vdb.json. The file might be corrupted.")?;

    // 2. Transform into the strict Intermediate Representation
    let ir_graph = payload.to_ir();

    // 3. Initialize Language Drivers (Hardcoding Go (Golang) for this phase)
    let driver = get_driver(TargetLanguage::Go);
    let merger = CodeMerger::new_go();

    // 4. Ensure the output directory exists
    let output_dir = "models";
    fs::create_dir_all(output_dir)?;

    // 5. Run the Generation & AST Merge Loop
    for entity in ir_graph.entities {
        // Format filename: "Users Table" -> "users_table.go"
        let safe_name = entity.name.to_lowercase().replace(" ", "_");
        let filename = format!("{}/{}.go", output_dir, safe_name);

        // Generate the strict target code
        let imports = driver.generate_imports(&entity);
        let struct_def = driver.generate_model(&entity);
        let combined_schema_code = format!("package models\n\n{}\n\n{}", imports, struct_def);

        // AST Pass: Rescue any custom developer code if the file already exists
        let custom_code = merger.extract_custom_zones(&filename);

        // AST Pass: Stitch the fresh schema with the rescued user logic
        merger.stitch_and_write(&filename, combined_schema_code, custom_code)?;
    }

    Ok(())
}
