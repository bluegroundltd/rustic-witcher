# Anonymization Configuration

## Introduction

In `rustic-witcher` the anonymization configuration is a TOML file that specifies how to anonymize the data.

## Configuration File

All configuration files can be found under the [configuration_data](configuration_data) directory. The configuration file is a TOML file that contains a list of anonymization rules. Each rule specifies how to anonymize a specific field in a table. The configuration file is loaded at runtime and is used to anonymize the data before they are stored in the database.

Each file is named after the following naming convention: `<database_name>-<schema_name>-sync.toml`.

If you don't want to anonymize any data for a given schema, you don't need to create a configuration file for it. There is a runtime check in `rustic-witcher`
that will skip the anonymization process if the configuration file is not found.

## Structure of configuration file

```toml
[[tables]]
name = "table_name"
[tables.anonymization_type]
type = "Multi" # or "Single"
```

If a table has multiple anonymization rules, a complete configuration section for the table will look like this:

```toml
[[tables]]
name = "table_name"
[tables.anonymization_type]
type = "Multi"

[[tables.anonymization_type.column_transformations]]
column_name = "column1"
[tables.anonymization_type.column_transformations.transformation_type]
type = "Custom"
operation_type = "fake_md5_transformation"

[[tables.anonymization_type.column_transformations]]
column_name = "column2"
[tables.anonymization_type.column_transformations.transformation_type]
type = "Custom"
operation_type = "fake_name_transformation"
```

If a table has only a single anonymization rule, a complete configuration section for the table will look like this:

```toml
[[tables]]
table_name = "table_name"
[tables.anonymization_type]
type = "Single"
transformation = "<transfomation_rule>" # Refer to the relevant documentation
```

## Transformation Types

### Faker transformation types
The following TOML configuration, accepts values from a predefined set of anonymization types:

```toml
[tables.anonymization_type.column_transformations.transformation_type]
type = "Custom"
operation_type = "<any_of_the_list_below>"
```

- `fake_phone_transformation`
- `fake_firstname_transformation`
- `fake_lastname_transformation`
- `fake_name_transformation`
- `fake_email_transformation`
- `fake_multi_email_transformation`
- `fake_companyname_transformation`
- `fake_address_transformation`
- `fake_md5_transformation`

### Replace transformation type

```toml
[tables.anonymization_type.column_transformations.transformation_type]
type = "Replace"
replacement_value = "<any_replacement_value>"
```

## Additional configuration options

### Reduced records for a table

In order to reduce the number of records to keep for a table, you can use the following configuration:

```toml
[tables.anonymization_type.column_transformations.transformation_type]
keep_num_of_records = <a_desired_number>
...
```

### Retain a value when it is empty

In order to retain a value when it is empty, you can use the following configuration:

```toml
[tables.anonymization_type.column_transformations.transformation_type]
retain_empty = true
...
```

### Filter a table based on values

1. Contains

```toml
[[tables]]
table_name = "some_table"
[tables.filter_type]
type = "Contains"
column = "a_column"
value = "some_contained_value"
```

2. StartsWith

```toml
[[tables]]
table_name = "some_table"
[tables.filter_type]
type = "StartsWith"
column = "a_column"
value = "starts_with_this_value"
```

2. EndsWith

```toml
[[tables]]
table_name = "some_table"
[tables.filter_type]
type = "EndsWith"
column = "a_column"
value = "ends_with_this_value"
```
