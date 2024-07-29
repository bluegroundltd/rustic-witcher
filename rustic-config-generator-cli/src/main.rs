use anyhow::Result;
use console::style;
use rustic_anonymization_config::config_structs::anonymization_config::AnonymizationConfig;
use rustic_anonymization_config::config_structs::column_transformation_struct::AnonymizationColumnTransformation;
use rustic_anonymization_config::config_structs::table_struct::AnonymizationConfigTable;
use rustic_anonymization_config::config_structs::table_type_struct::AnonymizationConfigTableType;
use rustic_anonymization_config::config_structs::transformation_type_struct::AnonymizationTransformationType;
use rustic_faker_types::FakerType;
use std::fs::{self, OpenOptions};
use std::io::prelude::*;
use strum::IntoEnumIterator;

use cliclack::{intro, outro, outro_note};

fn main() -> Result<()> {
    ctrlc::set_handler(move || {}).expect("setting Ctrl-C handler");

    cliclack::clear_screen()?;

    let available_configurations = fs::read_dir("../configuration_data")?;

    let mut available_configurations = available_configurations
        .filter_map(|entry| {
            entry.ok().and_then(|path| {
                if path.file_name().into_string().ok()?.ends_with(".toml") {
                    Some(path.path().display().to_string())
                } else {
                    None
                }
            })
        })
        .map(|path| {
            let path = path.replace("../configuration_data/", "");
            let path = path.replace("-sync.toml", "");
            (path.clone(), path, String::new())
        })
        .collect::<Vec<_>>();

    available_configurations.sort_by_key(|(key, _, _)| key.clone());

    intro(
        style(" Generate anonymization configuration! ")
            .on_cyan()
            .black(),
    )?;

    let configuration = cliclack::select("Select the configuration you want to use:")
        .items(&available_configurations)
        .interact()?;

    let table_name: String =
        cliclack::input("Which table do you want to generate config for?").interact()?;

    let column_name: String =
        cliclack::input("Which column do you want to generate config for?").interact()?;

    let operation_type =
        cliclack::select("Select the operation type you want to perform for the table:")
            .initial_value("replace")
            .item("replace", "Replace", "Will replace a value with another")
            .item(
                "custom",
                "Custom",
                "Pick one from the available custom operations:",
            )
            .interact()?;

    let custom_operation: Option<String> = if operation_type == "custom" {
        Some(build_custom_faker_type_selection())
    } else {
        None
    };

    let anonymization_transformation_type = if let Some(custom_operation) = custom_operation {
        AnonymizationTransformationType::Custom {
            operation_type: custom_operation,
        }
    } else {
        AnonymizationTransformationType::Replace {
            replacement_value: cliclack::input("Enter the replacement value:").interact()?,
        }
    };

    let anonymization_column_transformation = AnonymizationColumnTransformation {
        column_name,
        transformation_type: anonymization_transformation_type,
        retain_if_empty: None,
    };

    // Read the current configuration to check if we have a configuration entry for the selected table
    let config_file_name = format!("../configuration_data/{configuration}-sync.toml");
    let read_conf = fs::read_to_string(config_file_name.as_str());
    let mut config_under_edit = match read_conf {
        Ok(conf) => match toml::from_str(&conf) {
            Ok(conf) => conf,
            Err(e) => {
                panic!("Error parsing configuration file: {:?}", e);
            }
        },
        Err(_) => AnonymizationConfig::default(),
    };

    // Check if table already exists and if it does, append the new transformation
    let mut final_transformations = if let Some(table) = config_under_edit
        .tables
        .iter()
        .find(|table| table.table_name == table_name)
    {
        match &table.anonymization_type {
            AnonymizationConfigTableType::Multi {
                column_transformations,
            } => {
                let mut total_column_transformations: Vec<AnonymizationColumnTransformation> =
                    Vec::new();
                total_column_transformations.extend(column_transformations.clone());
                total_column_transformations
            }
            _ => {
                panic!("Table already exists with a different type")
            }
        }
    } else {
        vec![]
    };

    // These are the final transformations that will be pushed in the configuration
    final_transformations.push(anonymization_column_transformation);

    let anonymization_config_table_type = AnonymizationConfigTableType::Multi {
        column_transformations: final_transformations,
    };

    let anonymization_config_table = AnonymizationConfigTable {
        table_name: table_name.clone(),
        anonymization_type: anonymization_config_table_type,
        keep_num_of_records: None,
    };

    // Drop table if it exists
    config_under_edit
        .tables
        .retain(|table| table.table_name != table_name);

    // Then add the updated configuration
    config_under_edit.tables.push(anonymization_config_table);

    let formatted_config = toml::to_string(&config_under_edit).unwrap();

    _ = outro_note(
        format!("The following config will be appended to: {config_file_name}"),
        &formatted_config,
    );

    // Write the updated configuration to the file
    let mut file = OpenOptions::new()
        .write(true)
        .open(config_file_name)
        .unwrap();

    _ = file.write("#################################################\n".as_bytes());
    _ = file.write("###### Generated by rustic-config-generator #####\n".as_bytes());
    _ = file.write(formatted_config.as_bytes());
    _ = file.write("#################################################\n".as_bytes());

    // Do stuff
    outro("You're all set!")?;

    Ok(())
}

fn build_custom_faker_type_selection() -> String {
    let items_for_faker = FakerType::iter()
        .map(|faker_type| {
            let value = faker_type.to_string();
            let label = format!("{:?}", faker_type);
            (value, label, "")
        })
        .collect::<Vec<_>>();
    let items_for_faker = items_for_faker.as_slice();

    cliclack::select("Select Faker operation:")
        .items(items_for_faker)
        .interact()
        .unwrap()
}
