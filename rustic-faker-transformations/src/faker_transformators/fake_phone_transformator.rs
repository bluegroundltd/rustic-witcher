use bon::Builder;
use polars::prelude::*;
use rand::rngs::StdRng;
use rand::Rng;
use rustic_transformator::transformator::Transformator;
use rustic_transformator::transformator_output::TransformatorOutput;
use rustic_transformator::transformator_type::TransformatorType;

#[derive(Builder)]
pub struct FakePhoneTransformator {
    column_name: String,
}

impl Transformator for FakePhoneTransformator {
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
                        let transformed: String = value.chars().map(|c| {
                            if c.is_ascii_digit() {
                                let original_digit = c.to_digit(10).unwrap();
                                let mut new_digit = original_digit;
                                while new_digit == original_digit {
                                    new_digit = rng.random_range(0..10);
                                }
                                std::char::from_digit(new_digit, 10).unwrap()
                            } else {
                                c
                            }
                        }).collect();
                        Some(transformed)
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
