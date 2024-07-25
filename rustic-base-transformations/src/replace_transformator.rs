use polars::prelude::*;
use rand::rngs::StdRng;
use rustic_transformator::transformator::Transformator;
use rustic_transformator::transformator_output::TransformatorOutput;
use rustic_transformator::transformator_type::TransformatorType;

pub struct ReplaceTransformator {
    replacement_value: String,
    column_name: String,
}

impl ReplaceTransformator {
    pub fn new(column_name: impl Into<String>, replacement_value: impl Into<String>) -> Self {
        Self {
            column_name: column_name.into(),
            replacement_value: replacement_value.into(),
        }
    }
}

impl Transformator for ReplaceTransformator {
    fn transform(&self, input: &DataFrame, _: &mut StdRng) -> Vec<TransformatorOutput> {
        let fake_values = input.column(&self.column_name).unwrap().len();
        let fake_values = (0..fake_values)
            .map(|_| self.replacement_value.clone())
            .collect::<Vec<String>>();

        let series = Series::new(&self.column_name, fake_values);

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
    use rand::rngs::StdRng;
    use rand::SeedableRng;

    #[test]
    fn test_replace_transformator() {
        let df = DataFrame::new(vec![Series::new("a", &[1, 2, 3, 4, 5])]).unwrap();
        let transformator = ReplaceTransformator::new("a".to_string(), "test".to_string());
        let mut rng = StdRng::seed_from_u64(42);

        let transformed = transformator.transform(&df, &mut rng);

        assert_eq!(transformed.len(), 1);
        assert_eq!(transformed[0].column_name, "a");
        assert_eq!(transformed[0].series.len(), 5);
        assert!(transformed[0]
            .series
            .str()
            .unwrap()
            .into_iter()
            .all(|x| x.is_some() && x.unwrap() == "test"));
    }
}
