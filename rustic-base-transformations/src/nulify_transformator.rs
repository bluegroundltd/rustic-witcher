use polars::prelude::*;
use rand::rngs::StdRng;
use rustic_transformator::transformator::Transformator;
use rustic_transformator::transformator_output::TransformatorOutput;
use rustic_transformator::transformator_type::TransformatorType;

pub struct NullifyTransformator {
    column_name: String,
}

impl NullifyTransformator {
    pub fn new(column_name: impl Into<String>) -> Self {
        NullifyTransformator {
            column_name: column_name.into(),
        }
    }
}

impl Transformator for NullifyTransformator {
    fn transform(&self, input: &DataFrame, _: &mut StdRng) -> Vec<TransformatorOutput> {
        let column_name = self.column_name.clone();
        let col = input.column(&column_name).unwrap();
        let len = col.len();
        let dtype = col.dtype();

        let series = match dtype {
            DataType::String => Series::new(column_name.into(), vec![None::<String>; len]),
            DataType::Int32 => Series::new(column_name.into(), vec![None::<i32>; len]),
            DataType::Float64 => Series::new(column_name.into(), vec![None::<f64>; len]),
            _ => panic!("Unsupported data type: {:?}", dtype),
        };

        vec![TransformatorOutput {
            column_name: self.column_name.clone(),
            series,
        }]
    }

    fn transformator_type(&self) -> TransformatorType {
        TransformatorType::SingleColumn {
            column_name: self.column_name.clone(),
        }
    }
}

#[cfg(test)]
mod tests {

    use super::*;
    use rand::SeedableRng;
    use rand::rngs::StdRng;

    #[test]
    fn test_replace_transformator() {
        let df = DataFrame::new(vec![
            Series::new("a".into(), &["1", "2", "3", "4", "5"]).into(),
        ])
        .unwrap();
        let transformator = NullifyTransformator::new("a".to_string());
        let mut rng = StdRng::seed_from_u64(42);

        let transformed = transformator.transform(&df, &mut rng);

        assert_eq!(transformed.len(), 1);
        assert_eq!(transformed[0].column_name, "a");
        assert_eq!(transformed[0].series.len(), 5);
        assert!(
            transformed[0]
                .series
                .str()
                .unwrap()
                .into_iter()
                .all(|x| x.is_none())
        );
    }
}
