use bon::Builder;
use tracing::info;

use crate::config_structs::root_struct::Validations;
use std::{env, fs, path::PathBuf};

/// A struct for loading validation configurations.
#[derive(Builder)]
pub struct ValidationConfigLoader {
    database_name: String,
    schema_name: String,
}

impl ValidationConfigLoader {
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
