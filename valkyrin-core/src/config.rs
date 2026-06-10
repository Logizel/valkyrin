// valkyrin-core/src/config.rs
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

#[derive(Serialize, Deserialize)]
pub struct ValkyrinConfig {
    pub project_name: String,
    pub language: String,
    pub database_url_env: String,
}

impl Default for ValkyrinConfig {
    fn default() -> Self {
        Self {
            project_name: "my_backend_service".to_string(),
            language: "go".to_string(), // Can be changed to "python" or "rust"
            database_url_env: "DATABASE_URL".to_string(),
        }
    }
}

pub fn initialize_workspace() -> Result<()> {
    // 1. Create the valkyrin.yaml config file
    if !Path::new("valkyrin.yaml").exists() {
        let default_config = ValkyrinConfig::default();
        let yaml = serde_yaml::to_string(&default_config)?;
        fs::write("valkyrin.yaml", yaml)?;
        println!("📄 Created valkyrin.yaml");
    }

    // 2. Create an empty schema.vdb.json if it doesn't exist
    if !Path::new("schema.vdb.json").exists() {
        let empty_schema = r#"{"tables":[],"relations":[]}"#;
        fs::write("schema.vdb.json", empty_schema)?;
        println!("📄 Created schema.vdb.json");
    }

    // 3. Create the output directory
    fs::create_dir_all("models")?;
    println!("📁 Created models/ directory");

    Ok(())
}

pub fn load_config() -> Result<ValkyrinConfig> {
    let contents = fs::read_to_string("valkyrin.yaml")
        .unwrap_or_else(|_| "language: go\nproject_name: default".to_string());

    let config: ValkyrinConfig = serde_yaml::from_str(&contents)?;
    Ok(config)
}
