use std::path::PathBuf;
use std::{env, fs};
use tracing::debug;

use crate::config_structs::anonymization_config::AnonymizationConfig;
use crate::config_structs::table_struct::AnonymizationConfigTable;

impl AnonymizationConfig {
    /// Load the configuration for a specific database and schema.
    ///
    /// This method loads the configuration for a specific database and schema from a TOML file.
    ///
    /// # Arguments
    ///
    /// * `database_name` - The name of the database.
    /// * `schema_name` - The name of the schema.
    ///
    /// # Returns
    ///
    /// The `AnonymizationConfig` struct containing the loaded configuration.
    pub fn load_config_for(database_name: &str, schema_name: &str) -> AnonymizationConfig {
        let mut conf_file_path = PathBuf::new();
        conf_file_path.push(env::current_dir().unwrap());
        conf_file_path.push("configuration_data");
        conf_file_path.push(format!("{}-{}-sync.toml", database_name, schema_name));

        debug!("Configuration file path: {:?}", conf_file_path.as_os_str());

        let read_conf = fs::read_to_string(conf_file_path.as_os_str());

        match read_conf {
            Ok(conf) => match toml::from_str(&conf) {
                Ok(conf) => conf,
                Err(e) => {
                    panic!("Error parsing configuration file: {:?}", e);
                }
            },
            Err(_) => AnonymizationConfig::default(),
        }
    }

    /// Fetch the configuration for a specific table.
    ///
    /// This method fetches the configuration for a specific table from the loaded configuration.
    ///
    /// # Arguments
    ///
    /// * `table_name` - The name of the table.
    ///
    /// # Returns
    ///
    /// An `Option` containing a reference to the `AnonymizationConfigTable` if found, or `None` if not found.
    pub fn fetch_table_config(&self, table_name: &str) -> Option<&AnonymizationConfigTable> {
        self.tables
            .iter()
            .find(|table| table.table_name == table_name)
    }
}
