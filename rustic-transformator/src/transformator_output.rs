use bon::Builder;
use polars::prelude::*;

#[derive(Builder)]
pub struct TransformatorOutput {
    pub column_name: String,
    pub series: Series,
}
