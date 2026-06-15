// valkyrin-core/tests/property_roundtrip_tests.rs
//! Property‑based round‑trip sanity checks for all code‑generation drivers.
//! Guarantees that random IR entities never cause panics and always emit non‑empty source.

use proptest::collection::vec;
use proptest::prelude::*;
use proptest::string::string_regex;
use valkyrin_core::codegen::*;
use valkyrin_core::ir::{DataType, Entity, Field, IntSize, Constraints};

fn arb_int_size() -> impl Strategy<Value = IntSize> {
    prop_oneof![
        Just(IntSize::Small),
        Just(IntSize::Standard),
        Just(IntSize::Big),
    ]
}

fn arb_data_type() -> impl Strategy<Value = DataType> {
    let string_type = any::<Option<u32>>().prop_map(|max| DataType::String { max_length: max });
    let integer_type = arb_int_size().prop_map(DataType::Integer);
    let decimal_type = (0u8..=38, 0u8..=38)
        .prop_filter("scale <= precision", |(p, s)| s <= p)
        .prop_map(|(p, s)| DataType::Decimal { precision: p, scale: s });
    let enum_type = vec(string_regex("[a-z]{3,8}").unwrap(), 1..4).prop_map(DataType::Enum);
    prop_oneof![
        string_type,
        Just(DataType::Text),
        integer_type,
        Just(DataType::Float),
        decimal_type,
        Just(DataType::Boolean),
        Just(DataType::DateTime),
        Just(DataType::Json),
        Just(DataType::Uuid),
        enum_type,
    ]
}

fn arb_constraints() -> impl Strategy<Value = Constraints> {
    (
        any::<bool>(), // primary key
        any::<bool>(), // unique
        any::<bool>(), // nullable
        any::<bool>(), // indexed
        any::<Option<String>>(), // default value
    )
        .prop_map(|(pk, unique, nullable, indexed, default)| Constraints {
            is_primary_key: pk,
            is_unique: unique,
            is_nullable: nullable,
            is_indexed: indexed,
            default_value: default,
        })
}

fn arb_field() -> impl Strategy<Value = Field> {
    (
        string_regex("[a-z]{3,8}").unwrap(),
        arb_data_type(),
        arb_constraints(),
    )
        .prop_map(|(name, dt, cons)| Field {
            id: uuid::Uuid::new_v4().to_string(),
            name,
            data_type: dt,
            constraints: cons,
        })
}

fn arb_entity() -> impl Strategy<Value = Entity> {
    (
        string_regex("[A-Z][a-z]{3,8}").unwrap(),
        vec(arb_field(), 1..5),
    )
        .prop_map(|(name, fields)| Entity {
            id: uuid::Uuid::new_v4().to_string(),
            name,
            fields,
        })
}

proptest! {
    #[test]
    fn drivers_generate_nonempty_output(driver_idx in 0u8..9, entity in arb_entity()) {
        let driver: Box<dyn LanguageDriver> = match driver_idx % 9 {
            0 => Box::new(GoGormDriver),
            1 => Box::new(PythonSqlModelDriver),
            2 => Box::new(GoEntDriver),
            3 => Box::new(PythonSqlAlchemyDriver),
            4 => Box::new(RustDieselDriver),
            5 => Box::new(RustSeaOrmDriver),
            6 => Box::new(JavaScriptSequelizeDriver),
            7 => Box::new(TypeScriptPrismaDriver),
            _ => Box::new(TypeScriptTypeOrmDriver),
        };
        let output = driver.generate_model(&entity);
        prop_assert!(!output.trim().is_empty(), "Driver {} emitted empty output", driver.file_extension());
    }
}
