// valkyrin-core/src/compiler.rs
use crate::ast::CodeMerger;
use crate::canvas::CanvasPayload;
use crate::codegen::{TargetBackend, get_driver};
use crate::config::load_config;
use anyhow::{Context, Result};
use std::fs;

pub fn compile_blueprint() -> Result<()> {
    // 1. Load the configuration file
    let config = load_config()?;

    let file_contents = fs::read_to_string("schema.vdb.json")
        .context("Could not find schema.vdb.json. Have you saved the canvas yet?")?;

    let payload: CanvasPayload =
        serde_json::from_str(&file_contents).context("Failed to parse schema.vdb.json")?;

    let mut ir_graph = payload.to_ir();

     // Pass 2: The Relational Constraint Injector
     // This evaluates graph edges and injects Foreign Keys where needed
     let connections = ir_graph.connections.clone();
     for conn in connections {
         // Find the name and PK data_type of the Source Table
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
                     .unwrap_or(crate::ir::DataType::Uuid); // Fallback if PK not found
                 (e.name.clone(), pk_data_type)
             })
             .unwrap_or((String::new(), crate::ir::DataType::Uuid));

         // Inject the Foreign Key into the Target Table
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
                      data_type: source_pk_data_type, // Match the referenced PK's type
                      constraints: crate::ir::Constraints {
                          is_primary_key: false,
                          is_unique: false,
                          is_nullable: true,
                          is_indexed: false,
                          default_value: None,
                      },
                  });
             }
         }
     }

    // Map the yaml string and orm to our TargetBackend enum
    let target_backend = match (
        config.language.to_lowercase().as_str(),
        config.orm.as_ref().map(|s| s.to_lowercase()).as_deref(),
    ) {
        ("python", Some("sqlalchemy")) => TargetBackend::PythonSqlAlchemy,
        ("python", _) => TargetBackend::PythonSqlModel, // default to SQLModel
        ("rust", Some("diesel")) => TargetBackend::RustDiesel,
        ("rust", _) => TargetBackend::RustSeaOrm, // default to SeaORM
        ("typescript", Some("prisma")) => TargetBackend::TypeScriptPrisma,
        ("typescript", _) => TargetBackend::TypeScriptTypeOrm, // default to TypeORM
        ("javascript", Some("typeorm")) => TargetBackend::JavaScriptTypeOrm,
        ("javascript", _) => TargetBackend::JavaScriptSequelize, // default to Sequelize
        ("go", Some("ent")) => TargetBackend::GoEnt,
        ("go", _) => TargetBackend::GoGorm, // default to GORM
        _ => TargetBackend::GoGorm, // Global default
    };

     // Initialize Language Drivers dynamically based on backend
     let driver = get_driver(target_backend)?;

     let output_dir = "models";
     fs::create_dir_all(output_dir)?;

     // Collect generated entity names for diff-and-prune
     let mut generated_entity_names = std::collections::HashSet::new();

     // Pass 3: Run Generation & AST Merge Loop
     for entity in ir_graph.entities {
         let safe_name = entity.name.to_lowercase().replace(" ", "_");
         generated_entity_names.insert(safe_name.clone());
         
         // Get file extension from driver
         let ext = driver.file_extension();
         let filename = format!("{}/{}.{}", output_dir, safe_name, ext);

         // Driver's generate_model() now includes imports
         let generated_code = driver.generate_model(&entity);

         // Create merger and extract custom code
         let merger = CodeMerger::new(&target_backend);
         let custom_code = merger.extract_custom_zones(&filename, &target_backend);

         merger.stitch_and_write(&filename, generated_code, custom_code)?;
     }

     // Diff-and-prune: Delete orphaned files in models/ with managed extensions
     let managed_extensions = vec!["go", "py", "rs", "ts", "js", "prisma"];
     if let Ok(entries) = fs::read_dir(output_dir) {
         for entry in entries {
             if let Ok(entry) = entry {
                 let path = entry.path();
                 if path.is_file() {
                     if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
                         if managed_extensions.contains(&ext) {
                             if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                                 // Delete file if its stem is not in generated_entity_names
                                 if !generated_entity_names.contains(stem) {
                                     fs::remove_file(&path)?;
                                 }
                             }
                         }
                     }
                 }
             }
         }
     }

     Ok(())
}
