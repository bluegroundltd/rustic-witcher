use serde::{Deserialize, Serialize};

use super::transformation_type_struct::AnonymizationTransformationType;

#[cfg_attr(test, derive(Debug, PartialEq))]
#[derive(Serialize, Deserialize, Clone)]
#[allow(dead_code)]
pub struct AnonymizationColumnTransformation {
    pub column_name: String,
    pub transformation_type: AnonymizationTransformationType,
    pub retain_if_empty: Option<bool>,
}
