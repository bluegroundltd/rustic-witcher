use serde::{Deserialize, Serialize};

use super::validation_struct::ValidationConfiguration;

#[cfg_attr(test, derive(Debug, PartialEq))]
#[derive(Serialize, Deserialize, Clone, Default)]
#[allow(dead_code)]
pub struct Validations {
    pub validations: Vec<ValidationConfiguration>,
}
