use serde::{Deserialize, Serialize};

use super::value_check_type_struct::ValueCheckType;

#[cfg_attr(test, derive(PartialEq))]
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ValidationConfiguration {
    pub table: String,
    pub query: String,
    pub column_to_check: String,
    pub value_check_type: ValueCheckType,
}
