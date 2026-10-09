use polars::prelude::*;

pub struct TransformatorOutput {
    pub column_name: String,
    pub series: Series,
}
