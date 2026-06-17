// valkyrin-core/src/error.rs
use std::fmt;
use thiserror::Error;

/// Structured error types for Valkyrin with error codes for CI/CD integration
#[derive(Error, Debug, Clone)]
pub enum ValkyrinError {
    #[error("VAL-001: Configuration error - {0}")]
    Config(String),

    #[error("VAL-002: Schema validation error - {0}")]
    Schema(String),

    #[error("VAL-003: Database connection error - {0}")]
    Database(String),

    #[error("VAL-004: Migration error - {0}")]
    Migration(String),

    #[error("VAL-005: Code generation error - {0}")]
    Codegen(String),

    #[error("VAL-006: File I/O error - {0}")]
    Io(String),

    #[error("VAL-007: Parsing error - {0}")]
    Parse(String),

    #[error("VAL-008: Validation error - {0}")]
    Validation(String),

    #[error("VAL-009: Introspection error - {0}")]
    Introspection(String),

    #[error("VAL-010: Sync error - {0}")]
    Sync(String),

    #[error("VAL-011: CLI argument error - {0}")]
    CliArg(String),

    #[error("VAL-012: Internal error - {0}")]
    Internal(String),
}

impl ValkyrinError {
    /// Get the error code (e.g., "VAL-001")
    pub fn code(&self) -> &'static str {
        match self {
            ValkyrinError::Config(_) => "VAL-001",
            ValkyrinError::Schema(_) => "VAL-002",
            ValkyrinError::Database(_) => "VAL-003",
            ValkyrinError::Migration(_) => "VAL-004",
            ValkyrinError::Codegen(_) => "VAL-005",
            ValkyrinError::Io(_) => "VAL-006",
            ValkyrinError::Parse(_) => "VAL-007",
            ValkyrinError::Validation(_) => "VAL-008",
            ValkyrinError::Introspection(_) => "VAL-009",
            ValkyrinError::Sync(_) => "VAL-010",
            ValkyrinError::CliArg(_) => "VAL-011",
            ValkyrinError::Internal(_) => "VAL-012",
        }
    }

    /// Get the exit code for this error type
    pub fn exit_code(&self) -> i32 {
        match self {
            ValkyrinError::Validation(_) => 1,  // Warning
            _ => 2,  // Error
        }
    }

    /// Convert to JSON string for machine-readable output
    pub fn to_json(&self) -> String {
        serde_json::json!({
            "code": self.code(),
            "message": self.to_string(),
            "exit_code": self.exit_code(),
        }).to_string()
    }
}

/// Result type alias for Valkyrin operations
pub type ValkyrinResult<T> = Result<T, ValkyrinError>;

/// Convert anyhow::Error to ValkyrinError
pub fn from_anyhow(err: anyhow::Error) -> ValkyrinError {
    ValkyrinError::Internal(err.to_string())
}

/// Convert std::io::Error to ValkyrinError
pub fn from_io(err: std::io::Error) -> ValkyrinError {
    ValkyrinError::Io(err.to_string())
}

/// Convert serde_json::Error to ValkyrinError
pub fn from_serde_json(err: serde_json::Error) -> ValkyrinError {
    ValkyrinError::Parse(err.to_string())
}

/// Convert sqlx::Error to ValkyrinError
pub fn from_sqlx(err: sqlx::Error) -> ValkyrinError {
    ValkyrinError::Database(err.to_string())
}

impl From<std::io::Error> for ValkyrinError {
    fn from(err: std::io::Error) -> Self {
        ValkyrinError::Io(err.to_string())
    }
}

impl From<sqlx::Error> for ValkyrinError {
    fn from(err: sqlx::Error) -> Self {
        ValkyrinError::Database(err.to_string())
    }
}

impl From<serde_json::Error> for ValkyrinError {
    fn from(err: serde_json::Error) -> Self {
        ValkyrinError::Parse(err.to_string())
    }
}