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
    StartsAndEndsWith {
        column: String,
        start_value: String,
        end_value: String,
    },
    Equals {
        column: String,
        value: String,
    },
    AnyOfInt {
        column: String,
        values: Vec<i32>,
    },
    AnyOfString {
        column: String,
        values: Vec<String>,
    },
    #[serde(other)]
    #[default]
    NoFilter,
}
