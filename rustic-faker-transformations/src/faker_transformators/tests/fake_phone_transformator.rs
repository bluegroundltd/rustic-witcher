#[cfg(test)]
mod tests {

    use polars::prelude::*;
    use pretty_assertions::assert_eq;
    use rand::{SeedableRng, rngs::StdRng};
    use rustic_transformator::transformator::Transformator;

    use crate::faker_transformators::fake_phone_transformator::FakePhoneTransformator;

    #[test]
    fn test_fake_phone_transformator() {
        let original_phone_number = "+44 20 7123 4567";
        let df = DataFrame::new(vec![
            Series::new("a".into(), &[original_phone_number.to_string()]).into(),
        ])
        .unwrap();
        let transformator = FakePhoneTransformator::builder()
            .column_name("a".to_string())
            .build();
        let mut rng = StdRng::seed_from_u64(42);

        let transformed = transformator.transform(&df, &mut rng);

        assert_eq!(transformed.len(), 1);
        assert_eq!(transformed[0].column_name, "a");
        assert_eq!(transformed[0].series.len(), 1);
        assert_eq!(
            transformed[0]
                .series
                .str()
                .unwrap()
                .into_iter()
                .all(|x| x.is_some() && x.unwrap() != original_phone_number),
            true
        );
    }
}
