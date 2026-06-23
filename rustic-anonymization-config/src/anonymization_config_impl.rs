use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, OnceLock, RwLock};
use std::{env, fs};
use tracing::debug;

use crate::config_structs::anonymization_config::AnonymizationConfig;
use crate::config_structs::table_struct::AnonymizationConfigTable;

/// Process-wide cache of parsed configurations, keyed by `"{database}-{schema}"`.
///
/// The TOML config for a database/schema is immutable for the lifetime of a run,
/// but `load_config_for` is invoked once per Parquet file. Without caching, a table
/// with N LOAD files re-reads the file from disk and re-parses the TOML N times.
/// We parse once and hand out cheap `Arc` clones afterwards.
fn config_cache() -> &'static RwLock<HashMap<String, Arc<AnonymizationConfig>>> {
    static CACHE: OnceLock<RwLock<HashMap<String, Arc<AnonymizationConfig>>>> = OnceLock::new();
    CACHE.get_or_init(|| RwLock::new(HashMap::new()))
}

impl AnonymizationConfig {
    /// Load the configuration for a specific database and schema.
    ///
    /// This method loads the configuration for a specific database and schema from a TOML file.
    /// The result is cached process-wide, so subsequent calls for the same
    /// database/schema return a cheap `Arc` clone without touching the filesystem.
    ///
    /// # Arguments
    ///
    /// * `database_name` - The name of the database.
    /// * `schema_name` - The name of the schema.
    ///
    /// # Returns
    ///
    /// An `Arc` to the `AnonymizationConfig` struct containing the loaded configuration.
    pub fn load_config_for(database_name: &str, schema_name: &str) -> Arc<AnonymizationConfig> {
        let cache_key = format!("{database_name}-{schema_name}");

        // Fast path: already parsed.
        if let Some(config) = config_cache().read().unwrap().get(&cache_key) {
            return Arc::clone(config);
        }

        // Slow path: read + parse, then memoize.
        let config = Arc::new(Self::load_config_from_disk(database_name, schema_name));

        let mut cache = config_cache().write().unwrap();
        // Another thread may have inserted while we were parsing; keep whichever is present.
        Arc::clone(cache.entry(cache_key).or_insert(config))
    }

    /// Reads and parses the configuration file from disk, without caching.
    fn load_config_from_disk(database_name: &str, schema_name: &str) -> AnonymizationConfig {
        let mut conf_file_path = PathBuf::new();
        conf_file_path.push(env::current_dir().unwrap());
        conf_file_path.push("configuration_data");
        conf_file_path.push(format!("{database_name}-{schema_name}-sync.toml"));

        debug!("Configuration file path: {:?}", conf_file_path.as_os_str());

        let read_conf = fs::read_to_string(conf_file_path.as_os_str());

        match read_conf {
            Ok(conf) => match toml::from_str(&conf) {
                Ok(conf) => conf,
                Err(e) => {
                    panic!("Error parsing configuration file: {e:?}");
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
