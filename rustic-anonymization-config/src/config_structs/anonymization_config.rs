use serde::{Deserialize, Serialize};

use super::table_struct::AnonymizationConfigTable;

#[cfg_attr(test, derive(Debug, PartialEq))]
#[derive(Serialize, Deserialize, Default)]
pub struct AnonymizationConfig {
    pub tables: Vec<AnonymizationConfigTable>,
}
