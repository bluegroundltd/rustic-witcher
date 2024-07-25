#[cfg(test)]
mod tests {

    use crate::faker_transformators::FakeEmailTransformator;
    use polars::prelude::*;
    use pretty_assertions::assert_eq;
    use rand::{rngs::StdRng, SeedableRng};
    use rustic_transformator::transformator::Transformator;

    #[test]
    fn test_fake_email_transformator() {
        let df = DataFrame::new(vec![Series::new("a", &["foo-bar", "foo-qux"])]).unwrap();
        let transformator = FakeEmailTransformator::new("a".to_string(), false);
        let mut rng = StdRng::seed_from_u64(42);

        let transformed = transformator.transform(&df, &mut rng);

        assert_eq!(transformed.len(), 1);
        assert_eq!(transformed[0].column_name, "a");
        assert_eq!(transformed[0].series.len(), 2);
        assert_eq!(
            transformed[0]
                .series
                .str()
                .unwrap()
                .into_iter()
                .all(|x| x.is_some() && x.unwrap() != "foo-bar"),
            true
        );
    }
}
