use bon::Builder;
use polars::prelude::*;
use rand::rngs::StdRng;
use rustic_transformator::transformator_type::TransformatorType;

use fake::{Fake, faker::internet::raw::SafeEmail, locales::EN};
use rustic_transformator::transformator::Transformator;
use rustic_transformator::transformator_output::TransformatorOutput;

#[derive(Builder)]
pub struct FakeMultiEmailTransformator {
    column_name: String,
}

impl Transformator for FakeMultiEmailTransformator {
    fn transform(&self, input: &DataFrame, rng: &mut StdRng) -> Vec<TransformatorOutput> {
        let column_values = input
            .column(&self.column_name)
            .unwrap()
            .str()
            .unwrap()
            .iter()
            .map(|value| {
                if let Some(value) = value {
                    if value.len() > 1 {
                        let original_value_trimmed = &value[1..value.len() - 1];
                        let updated_value = original_value_trimmed
                            .split(',')
                            .map(|_| SafeEmail(EN).fake_with_rng(rng))
                            .collect::<Vec<String>>()
                            .join(",");
                        Some(format!("{{{updated_value}}}"))
                    } else {
                        Some(value.to_string())
                    }
                } else {
                    None
                }
            })
            .collect::<Vec<Option<String>>>();

        let transformed_series =
            StringChunked::new((&self.column_name).into(), column_values).into_series();

        vec![TransformatorOutput {
            column_name: self.column_name.to_string(),
            series: transformed_series,
        }]
    }

    fn transformator_type(&self) -> TransformatorType {
        TransformatorType::MultiColumn
    }
}
