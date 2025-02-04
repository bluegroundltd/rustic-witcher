use std::time::Instant;

use crate::transformator_output::TransformatorOutput;
use crate::transformator_type::TransformatorType;
use fake::{
    faker::{
        address::raw::{CityName, PostCode, StreetName, ZipCode},
        company::raw::CompanyName,
        internet::raw::SafeEmail,
        name::raw::{FirstName, LastName, Name},
        phone_number::raw::PhoneNumber,
    },
    locales::EN,
    uuid::UUIDv4,
    Fake,
};
use polars::prelude::*;
use rand::{rngs::StdRng, RngCore as _, SeedableRng};
use rand_seeder::{SipHasher, SipRng};
use rustic_duration::beautify_duration;
use rustic_faker_types::FakerType;
use tracing::info;

/// Generates a fake value using the specified faker type and RNG.
fn generate_fake_value_with_rng(faker: &FakerType, rng: &mut StdRng) -> String {
    match faker {
        FakerType::FirstName => FirstName(EN).fake_with_rng::<String, _>(rng),
        FakerType::LastName => LastName(EN).fake_with_rng::<String, _>(rng),
        FakerType::Name => Name(EN).fake_with_rng::<String, _>(rng),
        FakerType::CompanyName => CompanyName(EN).fake_with_rng::<String, _>(rng),
        FakerType::Email => SafeEmail(EN).fake_with_rng::<String, _>(rng),
        FakerType::PhoneNumber => PhoneNumber(EN).fake_with_rng::<String, _>(rng),
        FakerType::Address => {
            let city_name = CityName(EN).fake_with_rng::<String, _>(rng);
            let post_code = PostCode(EN).fake_with_rng::<String, _>(rng);
            let street_name = StreetName(EN).fake_with_rng::<String, _>(rng);
            let zip_code = ZipCode(EN).fake_with_rng::<String, _>(rng);
            format!("{} {}, {}, {}", street_name, zip_code, city_name, post_code)
        }
        FakerType::Md5 => UUIDv4.fake(),
    }
}

/// Generates a fake value using the specified faker type and random number generator.
fn generate_fake_value_without_rng(faker: &FakerType) -> String {
    match faker {
        FakerType::FirstName => FirstName(EN).fake::<String>(),
        FakerType::LastName => LastName(EN).fake::<String>(),
        FakerType::Name => Name(EN).fake::<String>(),
        FakerType::CompanyName => CompanyName(EN).fake::<String>(),
        FakerType::Email => SafeEmail(EN).fake::<String>(),
        FakerType::PhoneNumber => PhoneNumber(EN).fake::<String>(),
        FakerType::Address => {
            let city_name = CityName(EN).fake::<String>();
            let post_code = PostCode(EN).fake::<String>();
            let street_name = StreetName(EN).fake::<String>();
            let zip_code = ZipCode(EN).fake::<String>();
            format!("{} {}, {}, {}", street_name, zip_code, city_name, post_code)
        }
        FakerType::Md5 => UUIDv4.fake(),
    }
}

/// Represents a transformation operation.
pub trait Transformator: Send + Sync {
    /// Transforms the input DataFrame using the specified random number generator.
    fn transform(&self, input: &DataFrame, rng: &mut StdRng) -> Vec<TransformatorOutput>;

    /// Returns the type of transformation.
    fn transformator_type(&self) -> TransformatorType;

    /// Transforms the input DataFrame by generating fake values for the specified column using the specified faker type and random number generator.
    /// If `retain_if_empty` is true, the original values will be retained if they are empty or null.
    fn transform_with_faker(
        &self,
        input: &DataFrame,
        column_name: &str,
        rng: &mut StdRng,
        faker_type: FakerType,
        retain_if_empty: bool,
    ) -> Vec<TransformatorOutput> {
        let start = Instant::now();

        let series: Vec<Option<String>> = input
            .column(column_name)
            .unwrap()
            .str()
            .unwrap()
            .into_iter()
            .map(|value| {
                let rng = &mut rng.clone();
                match value {
                    Some(value) if retain_if_empty && value.is_empty() => Some(value.to_string()),
                    Some(value) => {
                        let value_seed = &mut SipHasher::from(value).into_rng();
                        let value_seed = generate_seed_from_sip_rng(value_seed);
                        let value_seed = &mut StdRng::from_seed(value_seed);
                        let mut value_seed = combine_seeds(rng, value_seed);
                        let rng = &mut StdRng::from_rng(&mut value_seed);
                        Some(generate_fake_value_with_rng(&faker_type, rng))
                    }
                    None if retain_if_empty => value.map(|value| value.to_string()),
                    _ => Some(generate_fake_value_without_rng(&faker_type)),
                }
            })
            .collect::<Vec<_>>();

        let elapsed = start.elapsed();

        info!(
            "Time elapsed from faker transform: {}",
            beautify_duration(elapsed)
        );

        vec![TransformatorOutput {
            column_name: column_name.to_string(),
            series: Series::new(column_name.into(), series),
        }]
    }
}

pub fn generate_seed_from_sip_rng(initial_seed: &mut SipRng) -> [u8; 32] {
    let mut seed = [0u8; 32];
    initial_seed.fill_bytes(&mut seed);
    seed
}

pub fn combine_seeds(rng1: &mut StdRng, rng2: &mut StdRng) -> StdRng {
    // Extract seeds from the two StdRng objects
    let mut seed1 = [0u8; 32];
    let mut seed2 = [0u8; 32];

    rng1.fill_bytes(&mut seed1);
    rng2.fill_bytes(&mut seed2);

    // Combine the seeds (for simplicity, here we concatenate the first 16 bytes from each)
    let combined_seed: [u8; 32] = [&seed1[0..16], &seed2[0..16]]
        .concat()
        .try_into()
        .expect("slice with incorrect length");

    // Create a new StdRng with the combined seed
    StdRng::from_seed(combined_seed)
}
