use tracing::info;

use crate::config_structs::root_struct::Validations;
use std::{env, fs, path::PathBuf};

/// A struct for loading validation configurations.
pub struct ValidationConfigLoader {
    pub database_name: String,
    pub schema_name: String,
}

impl ValidationConfigLoader {
    /// Creates a new `ValidationConfigLoader` instance.
    ///
    /// # Arguments
    ///
    /// * `database_name` - The name of the database.
    /// * `schema_name` - The name of the schema.
    ///
    /// # Example
    ///
    /// ```
    /// use crate::config_loader::ValidationConfigLoader;
    ///
    /// let loader = ValidationConfigLoader::new("my_database", "my_schema");
    /// ```
    pub fn new(database_name: impl Into<String>, schema_name: impl Into<String>) -> Self {
        Self {
            database_name: database_name.into(),
            schema_name: schema_name.into(),
        }
    }

    /// Loads the validations configuration.
    ///
    /// # Returns
    ///
    /// The loaded `Validations` struct.
    ///
    /// # Example
    ///
    /// ```
    /// use crate::config_loader::ValidationConfigLoader;
    /// use crate::config_structs::root_struct::Validations;
    ///
    /// let loader = ValidationConfigLoader::new("my_database", "my_schema");
    /// let validations = loader.load_validations_config();
    /// ```
    pub fn load_validations_config(&self) -> Validations {
        let mut conf_file_path = PathBuf::new();
        conf_file_path.push(env::current_dir().unwrap());
        conf_file_path.push("configuration_data");
        conf_file_path.push("validations");
        conf_file_path.push(format!("{}-{}.toml", self.database_name, self.schema_name));

        info!("Configuration file path: {:?}", conf_file_path.as_os_str());

        let read_conf = fs::read_to_string(conf_file_path.as_os_str());

        match read_conf {
            Ok(conf) => match toml::from_str(&conf) {
                Ok(conf) => conf,
                Err(e) => {
                    panic!("Error parsing configuration file: {:?}", e);
                }
            },
            Err(_) => Validations::default(),
        }
    }
}
