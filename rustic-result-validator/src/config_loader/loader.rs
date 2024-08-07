use tracing::info;

use crate::config_structs::root_struct::Validations;
use std::{env, fs, path::PathBuf};

pub struct ValidationConfigLoader {
    pub database_name: String,
    pub schema_name: String,
}

impl ValidationConfigLoader {
    pub fn new(database_name: impl Into<String>, schema_name: impl Into<String>) -> Self {
        Self {
            database_name: database_name.into(),
            schema_name: schema_name.into(),
        }
    }

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
