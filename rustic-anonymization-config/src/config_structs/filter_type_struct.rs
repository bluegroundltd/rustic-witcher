use serde::{Deserialize, Serialize};

#[cfg_attr(test, derive(Debug, PartialEq))]
#[derive(Serialize, Deserialize, Clone, Default)]
#[serde(tag = "type")]
pub enum FilterType {
    Contains {
        column: String,
        value: String,
    },
    StartsWith {
        column: String,
        value: String,
    },
    EndsWith {
        column: String,
        value: String,
    },
    #[default]
    NoFilter,
}
