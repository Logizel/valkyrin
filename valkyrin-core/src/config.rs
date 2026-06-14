// valkyrin-core/src/config.rs
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

#[derive(Serialize, Deserialize)]
pub struct ValkyrinConfig {
    pub language: String,
    pub orm: Option<String>,
    pub database_url_env: String,
}

impl Default for ValkyrinConfig {
    fn default() -> Self {
        Self {
            language: "go".to_string(),
            orm: Some("gorm".to_string()),
            database_url_env: "DATABASE_URL".to_string(),
        }
    }
}

/// Supported language + ORM combinations
const SUPPORTED_BACKENDS: &[(&str, &[&str])] = &[
    ("go", &["gorm", "ent"]),
    ("python", &["sqlmodel", "sqlalchemy"]),
    ("rust", &["seaorm", "diesel"]),
    ("typescript", &["typeorm", "prisma"]),
    ("javascript", &["sequelize", "typeorm"]),
];

pub fn initialize_workspace() -> Result<()> {
    if !Path::new("valkyrin.yaml").exists() {
        let default_config = ValkyrinConfig::default();
        let yaml = serde_yaml::to_string(&default_config)?;
        fs::write("valkyrin.yaml", yaml)?;
        println!("📄 Created valkyrin.yaml");
    }

    if !Path::new("schema.vdb.json").exists() {
        let empty_schema = r#"{"tables":[],"relations":[]}"#;
        fs::write("schema.vdb.json", empty_schema)?;
        println!("📄 Created schema.vdb.json");
    }

    fs::create_dir_all("models")?;
    println!("📁 Created models/ directory");

    Ok(())
}

pub fn load_config() -> Result<ValkyrinConfig> {
    let contents = fs::read_to_string("valkyrin.yaml")
        .context("valkyrin.yaml not found. Run 'valkyrin init' to create it.")?;

    let config: ValkyrinConfig = serde_yaml::from_str(&contents)
        .context("Failed to parse valkyrin.yaml. Ensure it is valid YAML.")?;

    // Validate language
    let lang = config.language.to_lowercase();
    let is_supported = SUPPORTED_BACKENDS.iter().any(|(l, _)| *l == lang);
    ensure!(
        is_supported,
        "Unsupported language '{}'. Supported languages: {}",
        config.language,
        SUPPORTED_BACKENDS
            .iter()
            .map(|(l, _)| *l)
            .collect::<Vec<_>>()
            .join(", ")
    );

    // Validate ORM if specified
    if let Some(ref orm) = config.orm {
        let orm_lower = orm.to_lowercase();
        let valid_or_forms: &[&str] = SUPPORTED_BACKENDS
            .iter()
            .find(|(l, _)| *l == lang)
            .map(|(_, orms)| *orms)
            .unwrap_or(&[]);

        let is_valid_orm = valid_or_forms.iter().any(|o| *o == orm_lower);
        ensure!(
            is_valid_orm,
            "Unsupported ORM '{}' for language '{}'. Supported ORMs: {}",
            orm,
            lang,
            valid_or_forms.join(", ")
        );
    }

    Ok(config)
}
