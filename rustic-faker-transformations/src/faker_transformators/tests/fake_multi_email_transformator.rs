#[cfg(test)]
mod tests {

    use crate::faker_transformators::fake_multi_email_transformator::FakeMultiEmailTransformator;
    use polars::prelude::*;
    use pretty_assertions::assert_eq;
    use rand::{rngs::StdRng, SeedableRng};
    use rustic_transformator::transformator::Transformator;

    #[test]
    fn test_transform() {
        let mut rng = StdRng::seed_from_u64(42);
        let fake_multi_email_transformator = FakeMultiEmailTransformator::builder()
            .column_name("a".to_string())
            .build();
        let df = DataFrame::new(vec![Series::new("a".into(), &["foo, bar, qux"]).into()]).unwrap();
        let transformed = fake_multi_email_transformator.transform(&df, &mut rng);

        assert_eq!(transformed.len(), 1);
        assert_eq!(transformed[0].column_name, "a");
        assert_eq!(transformed[0].series.len(), 1);

        let transformed_output = transformed[0].series.str().unwrap().get(0).unwrap();

        assert_eq!(transformed_output.starts_with('{'), true);
        assert_eq!(transformed_output.ends_with('}'), true);
        assert_eq!(transformed_output.split(',').collect::<Vec<_>>().len(), 3);
    }
}
