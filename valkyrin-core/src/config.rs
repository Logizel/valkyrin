// valkyrin-core/src/config.rs
use anyhow::{Context, Result, ensure};
use dotenvy::dotenv;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::Path;

#[derive(Serialize, Deserialize, Default)]
pub struct EnvironmentConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub database_url_env: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output_dir: Option<String>,
}

#[derive(Serialize, Deserialize)]
pub struct ValkyrinConfig {
    pub language: String,
    pub orm: Option<String>,
    pub database_url_env: String,
    #[serde(default)]
    pub environments: HashMap<String, EnvironmentConfig>,
}

impl Default for ValkyrinConfig {
    fn default() -> Self {
        Self {
            language: "go".to_string(),
            orm: Some("gorm".to_string()),
            database_url_env: "DATABASE_URL".to_string(),
            environments: HashMap::new(),
        }
    }
}

impl ValkyrinConfig {
    /// Returns the default config with example environments for init
    pub fn default_with_examples() -> Self {
        let mut envs = HashMap::new();
        envs.insert(
            "dev".to_string(),
            EnvironmentConfig {
                database_url_env: Some("DATABASE_URL_DEV".to_string()),
                output_dir: Some("./models/dev".to_string()),
            },
        );
        envs.insert(
            "staging".to_string(),
            EnvironmentConfig {
                database_url_env: Some("DATABASE_URL_STAGING".to_string()),
                output_dir: Some("./models/staging".to_string()),
            },
        );
        envs.insert(
            "prod".to_string(),
            EnvironmentConfig {
                database_url_env: Some("DATABASE_URL_PROD".to_string()),
                output_dir: Some("./models/prod".to_string()),
            },
        );
        Self {
            language: "go".to_string(),
            orm: Some("gorm".to_string()),
            database_url_env: "DATABASE_URL".to_string(),
            environments: envs,
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
    dotenv().ok();
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
    dotenv().ok();
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

/// Resolves the database URL for a given environment.
/// Resolution order:
/// 1. Environment-specific .env.<env> file (if env specified)
/// 2. Environment-specific database_url_env from valkyrin.yaml
/// 3. Global database_url_env from valkyrin.yaml
/// 4. .env file
/// 5. Default: "DATABASE_URL"
pub fn resolve_database_url(config: &ValkyrinConfig, env: Option<&str>) -> String {
    let env_name = env.unwrap_or("default");

    // If specific environment requested, try to load .env.<env>env>
    if let Some(env_str) = env {
        let env_file = format!(".env.{}", env_str);
        if Path::new(&env_file).exists() {
            dotenvy::from_filename(&env_file).ok();
        }
    }

    // Determine which env var name to use
    let db_url_env = if let Some(env_str) = env {
        config
            .environments
            .get(env_str)
            .and_then(|e| e.database_url_env.as_deref())
            .unwrap_or(&config.database_url_env)
    } else {
        &config.database_url_env
    };

    std::env::var(db_url_env).unwrap_or_else(|_| {
        if db_url_env == "DATABASE_URL" {
            "postgresql://localhost/dbname".to_string()
        } else {
            format!("${}", db_url_env)
        }
    })
}

/// Resolves the output directory for a given environment.
pub fn resolve_output_dir(config: &ValkyrinConfig, env: Option<&str>) -> String {
    if let Some(env_str) = env {
        config
            .environments
            .get(env_str)
            .and_then(|e| e.output_dir.as_deref())
            .unwrap_or("models")
    } else {
        "models"
    }
    .to_string()
}
