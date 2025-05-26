use bon::Builder;
use fake::Fake;
use fake::faker::internet::raw::SafeEmail;
use fake::locales::EN;
use polars::prelude::*;
use rand::{SeedableRng, rngs::StdRng};
use rand_seeder::SipHasher;
use rustic_transformator::transformator::{
    Transformator, combine_seeds, generate_seed_from_sip_rng,
};
use rustic_transformator::transformator_output::TransformatorOutput;
use rustic_transformator::transformator_type::TransformatorType;

#[derive(Builder)]
pub struct FakeEmailWithIdPrefixTransformator {
    column_name: String,
}

impl Transformator for FakeEmailWithIdPrefixTransformator {
    fn transform(&self, input: &DataFrame, initial: &mut StdRng) -> Vec<TransformatorOutput> {
        let user_id_series = &input
            .column("id")
            .expect("id not found")
            .as_series()
            .expect("id not found");

        let user_email_iter = input
            .column(&self.column_name)
            .unwrap()
            .str()
            .unwrap()
            .into_iter();

        let user_id = user_id_series.i32().unwrap();

        let transformed_values: Vec<Option<String>> = user_email_iter
            .zip(user_id)
            .map(|(email, user_id)| {
                let email = email.unwrap_or("");
                let email_seed = &mut SipHasher::from(email).into_rng();
                let email_seed = generate_seed_from_sip_rng(email_seed);
                let email_seed = &mut StdRng::from_seed(email_seed);
                let mut email_seed = combine_seeds(initial, email_seed);
                let rng = &mut StdRng::from_rng(&mut email_seed);
                let fake_email = SafeEmail(EN).fake_with_rng::<String, _>(rng);
                Some(format!("{}-{}", user_id.unwrap(), fake_email))
            })
            .collect();

        let transformed_series =
            StringChunked::new((&self.column_name).into(), transformed_values).into_series();

        vec![TransformatorOutput {
            column_name: self.column_name.to_string(),
            series: transformed_series,
        }]
    }

    fn transformator_type(&self) -> TransformatorType {
        TransformatorType::MultiColumn
    }
}
