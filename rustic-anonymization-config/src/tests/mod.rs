use crate::config_structs::{
    anonymization_config::AnonymizationConfig, table_type_struct::AnonymizationConfigTableType,
    transformation_type_struct::AnonymizationTransformationType,
};

#[test]
fn test_deserialize_config() {
    let config = r#"
        [[tables]]
        table_name = "table1"
        keep_num_of_records = 10_000
        [tables.anonymization_type]
        type = "Multi"

        [[tables.anonymization_type.column_transformations]]
        column_name = "column1"
        [tables.anonymization_type.column_transformations.transformation_type]
        type = "Replace"
        replacement_value = "replacement_value"

        [[tables.anonymization_type.column_transformations]]
        column_name = "column2"
        retain_if_empty = true
        [tables.anonymization_type.column_transformations.transformation_type]
        type = "Custom"
        operation_type = "operation_type"
    "#;

    let config: AnonymizationConfig = toml::from_str(config).unwrap();
    assert_eq!(config.tables.len(), 1);
    assert_eq!(config.tables[0].table_name, "table1");
    assert_eq!(config.tables[0].keep_num_of_records.unwrap(), 10_000);

    assert!(matches!(
        config.tables[0].anonymization_type,
        AnonymizationConfigTableType::Multi { .. }
    ));

    if let AnonymizationConfigTableType::Multi {
        column_transformations,
    } = &config.tables[0].anonymization_type
    {
        assert_eq!(column_transformations.len(), 2);
        assert_eq!(column_transformations[0].column_name, "column1");
        assert_eq!(
            column_transformations[0].transformation_type,
            AnonymizationTransformationType::Replace {
                replacement_value: "replacement_value".to_string()
            }
        );
        assert_eq!(column_transformations[1].column_name, "column2");
        assert_eq!(
            column_transformations[1].transformation_type,
            AnonymizationTransformationType::Custom {
                operation_type: "operation_type".to_string()
            }
        );
        assert!(column_transformations[1].retain_if_empty.unwrap());
    } else {
        panic!("Expected Multi type")
    }
}

#[test]
fn test_deserialize_config_with_specific_fake_operation() {
    let config = r#"
        [[tables]]
        table_name = "table1"
        [tables.anonymization_type]
        type = "Single"
        transformation = "fake_phone_transformation"
    "#;

    let config: AnonymizationConfig = toml::from_str(config).unwrap();
    assert_eq!(config.tables.len(), 1);
    assert_eq!(config.tables[0].table_name, "table1");

    let table_config = config.fetch_table_config("table1");

    assert!(table_config.is_some());
    assert!(matches!(
        table_config.unwrap().anonymization_type,
        AnonymizationConfigTableType::Single { .. }
    ));

    if let AnonymizationConfigTableType::Single { transformation } =
        &table_config.unwrap().anonymization_type
    {
        assert_eq!(transformation, "fake_phone_transformation");
    } else {
        panic!("Expected Single type")
    }
}
