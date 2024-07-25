use serde::{Deserialize, Serialize};

use super::table_type_struct::AnonymizationConfigTableType;

#[cfg_attr(test, derive(Debug, PartialEq))]
#[derive(Serialize, Deserialize)]
pub struct AnonymizationConfigTable {
    pub table_name: String,
    pub anonymization_type: AnonymizationConfigTableType,
    pub keep_num_of_records: Option<usize>,
}
