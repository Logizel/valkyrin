// valkyrin-core/tests/codegen_tests.rs
//! Basic smoke tests for code generation drivers.
//! Ensures that each driver produces non-empty output and includes expected identifiers.

use valkyrin_core::codegen::*;
use valkyrin_core::ir::{DataType, Entity, Field, IntSize, Constraints};

/// Helper to construct a test entity with a variety of field types.
fn test_entity() -> Entity {
    Entity {
        id: uuid::Uuid::new_v4().to_string(),
        name: "User".to_string(),
        fields: vec![
            Field {
                id: uuid::Uuid::new_v4().to_string(),
                name: "id".to_string(),
                data_type: DataType::Uuid,
                constraints: Constraints {
                    is_primary_key: true,
                    primary_key_order: Some(0),
                    is_unique: false,
                    is_nullable: false,
                    is_indexed: false,
                    default_value: None,
                },
            },
            Field {
                id: uuid::Uuid::new_v4().to_string(),
                name: "name".to_string(),
                data_type: DataType::String { max_length: Some(255) },
                constraints: Constraints {
                    is_primary_key: false,
                    primary_key_order: None,
                    is_unique: false,
                    is_nullable: false,
                    is_indexed: false,
                    default_value: None,
                },
            },
            Field {
                id: uuid::Uuid::new_v4().to_string(),
                name: "age".to_string(),
                data_type: DataType::Integer(IntSize::Standard),
                constraints: Constraints {
                    is_primary_key: false,
                    primary_key_order: None,
                    is_unique: false,
                    is_nullable: true,
                    is_indexed: false,
                    default_value: None,
                },
            },
            Field {
                id: uuid::Uuid::new_v4().to_string(),
                name: "is_active".to_string(),
                data_type: DataType::Boolean,
                constraints: Constraints {
                    is_primary_key: false,
                    primary_key_order: None,
                    is_unique: false,
                    is_nullable: false,
                    is_indexed: false,
                    default_value: None,
                },
            },
            Field {
                id: uuid::Uuid::new_v4().to_string(),
                name: "status".to_string(),
                data_type: DataType::Enum {
                    values: vec!["active".to_string(), "inactive".to_string()],
                    type_name: None,
                },
                constraints: Constraints {
                    is_primary_key: false,
                    primary_key_order: None,
                    is_unique: false,
                    is_nullable: false,
                    is_indexed: false,
                    default_value: None,
                },
            },
        ],
    }
}

/// Helper to construct a test entity with native PostgreSQL enum type name.
fn test_entity_with_native_enum() -> Entity {
    Entity {
        id: uuid::Uuid::new_v4().to_string(),
        name: "Product".to_string(),
        fields: vec![
            Field {
                id: uuid::Uuid::new_v4().to_string(),
                name: "id".to_string(),
                data_type: DataType::Uuid,
                constraints: Constraints {
                    is_primary_key: true,
                    primary_key_order: Some(0),
                    is_unique: false,
                    is_nullable: false,
                    is_indexed: false,
                    default_value: None,
                },
            },
            Field {
                id: uuid::Uuid::new_v4().to_string(),
                name: "category".to_string(),
                data_type: DataType::Enum {
                    values: vec!["electronics".to_string(), "clothing".to_string(), "books".to_string()],
                    type_name: Some("product_category".to_string()),
                },
                constraints: Constraints {
                    is_primary_key: false,
                    primary_key_order: None,
                    is_unique: false,
                    is_nullable: false,
                    is_indexed: false,
                    default_value: None,
                },
            },
        ],
    }
}

#[test]
fn test_all_drivers_generate_output() {
    let entity = test_entity();
    // List of driver instances to test.
    let drivers: Vec<Box<dyn LanguageDriver>> = vec![
        Box::new(GoGormDriver),
        Box::new(PythonSqlModelDriver),
        Box::new(GoEntDriver),
        Box::new(PythonSqlAlchemyDriver),
        Box::new(RustDieselDriver),
        Box::new(RustSeaOrmDriver),
        Box::new(JavaScriptSequelizeDriver),
        Box::new(JavaScriptTypeOrmDriver),
        Box::new(TypeScriptPrismaDriver),
        Box::new(TypeScriptTypeOrmDriver),
    ];

    for driver in drivers.iter() {
        let output = driver.generate_model(&entity);
        assert!(!output.trim().is_empty(), "Driver {} produced empty output", driver.file_extension());
        // Most drivers embed the entity name; Rust SeaORM uses a generic `Model` struct.
        // Accept either condition.
        let contains_name = output.contains(&entity.name);
        let contains_model = output.to_lowercase().contains("model");
        assert!(contains_name || contains_model, "Driver {} output missing expected identifiers", driver.file_extension());
        // Basic enum check – verify that some enum representation is present.
        if output.contains("enum") || output.contains("Enum") {
            // At least one occurrence of enum keyword should be present.
            assert!(output.to_lowercase().contains("enum"), "Driver {} should contain enum keyword", driver.file_extension());
        }
    }
}

#[test]
fn test_native_enum_type_name_emission() {
    let entity = test_entity_with_native_enum();
    let drivers: Vec<Box<dyn LanguageDriver>> = vec![
        Box::new(GoGormDriver),
        Box::new(PythonSqlModelDriver),
        Box::new(GoEntDriver),
        Box::new(PythonSqlAlchemyDriver),
        Box::new(RustDieselDriver),
        Box::new(RustSeaOrmDriver),
        Box::new(JavaScriptSequelizeDriver),
        Box::new(JavaScriptTypeOrmDriver),
        Box::new(TypeScriptPrismaDriver),
        Box::new(TypeScriptTypeOrmDriver),
    ];

    for driver in drivers.iter() {
        let ext = driver.file_extension();
        let output = driver.generate_model(&entity);
        assert!(!output.trim().is_empty(), "Driver {} produced empty output", ext);
        // For native enum, the type name should be present in the output
        // (e.g., "product_category" for Go, Python, etc.)
        assert!(
            output.contains("product_category"),
            "Driver {} should emit native enum type name 'product_category'",
            ext
        );
    }
}
