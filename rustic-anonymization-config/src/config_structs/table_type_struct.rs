use serde::{Deserialize, Serialize};

use super::column_transformation_struct::AnonymizationColumnTransformation;

#[cfg_attr(test, derive(Debug, PartialEq))]
#[derive(Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum AnonymizationConfigTableType {
    Multi {
        column_transformations: Vec<AnonymizationColumnTransformation>,
    },
    Single {
        transformation: String,
    },
}
