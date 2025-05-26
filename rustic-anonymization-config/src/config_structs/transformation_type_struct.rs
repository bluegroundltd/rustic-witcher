use serde::{Deserialize, Serialize};

#[cfg_attr(test, derive(Debug, PartialEq))]
#[derive(Serialize, Deserialize, Clone)]
#[serde(tag = "type")]
pub enum AnonymizationTransformationType {
    Replace { replacement_value: String },
    Custom { operation_type: String },
    Nullify,
}
