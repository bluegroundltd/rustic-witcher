use std::time::Instant;

use crate::transformator_output::TransformatorOutput;
use crate::transformator_type::TransformatorType;
use fake::{
    Fake,
    faker::{
        address::raw::{CityName, PostCode, StreetName, ZipCode},
        company::raw::CompanyName,
        internet::raw::SafeEmail,
        name::raw::{FirstName, LastName, Name},
        phone_number::raw::PhoneNumber,
    },
    locales::EN,
    uuid::UUIDv4,
};
use polars::prelude::*;
use rand::{RngCore as _, SeedableRng, rngs::StdRng};
use rand_seeder::{SipHasher, SipRng};
use rayon::prelude::*;
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
            format!("{street_name} {zip_code} {city_name} {post_code}")
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
            format!("{street_name} {zip_code} {city_name} {post_code}")
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

        // The caller's rng is never advanced: every row starts from a fresh clone
        // of it, so the 32 seed bytes it contributes are the same for all rows.
        // Draw them once instead of cloning and draining an rng per row.
        let mut seed1 = [0u8; 32];
        rng.clone().fill_bytes(&mut seed1);

        // Rows are independent (seeded per value), so fan them out over rayon.
        // ponytail: rayon's global pool, not Polars' — `polars` does not re-export
        // `polars_core::runtime::THREAD_POOL`, and Polars 0.54 would route a
        // non-Polars thread to its single-threaded no-op pool anyway. Both pools
        // size to the core count, so a concurrent scan decode oversubscribes.
        let series: Vec<Option<String>> = input
            .column(column_name)
            .unwrap()
            .str()
            .unwrap()
            .par_iter()
            .map(|value| match value {
                Some(value) if retain_if_empty && value.is_empty() => Some(value.to_string()),
                Some(value) => {
                    let value_seed = &mut SipHasher::from(value).into_rng();
                    let value_seed = generate_seed_from_sip_rng(value_seed);
                    let value_seed = &mut StdRng::from_seed(value_seed);
                    let mut value_seed = combine_seed_bytes(&seed1, value_seed);
                    let rng = &mut StdRng::from_rng(&mut value_seed);
                    Some(generate_fake_value_with_rng(&faker_type, rng))
                }
                None if retain_if_empty => None,
                _ => Some(generate_fake_value_without_rng(&faker_type)),
            })
            .collect();

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
    // Extract the seed from the first StdRng object
    let mut seed1 = [0u8; 32];
    rng1.fill_bytes(&mut seed1);

    combine_seed_bytes(&seed1, rng2)
}

/// `combine_seeds` with the first rng already drained into `seed1`.
fn combine_seed_bytes(seed1: &[u8; 32], rng2: &mut StdRng) -> StdRng {
    let mut seed2 = [0u8; 32];
    rng2.fill_bytes(&mut seed2);

    // Combine the seeds (for simplicity, here we concatenate the first 16 bytes from each)
    let combined_seed: [u8; 32] = [&seed1[0..16], &seed2[0..16]]
        .concat()
        .try_into()
        .expect("slice with incorrect length");

    // Create a new StdRng with the combined seed
    StdRng::from_seed(combined_seed)
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Faker;

    impl Transformator for Faker {
        fn transform(&self, _: &DataFrame, _: &mut StdRng) -> Vec<TransformatorOutput> {
            unreachable!()
        }

        fn transformator_type(&self) -> TransformatorType {
            TransformatorType::MultiColumn
        }
    }

    /// The previous per-row algorithm, kept verbatim as the golden reference:
    /// clone the caller's rng per row and combine it with the value's SipHash seed.
    fn reference(
        values: &StringChunked,
        rng: &mut StdRng,
        faker_type: &FakerType,
        retain_if_empty: bool,
    ) -> Vec<Option<String>> {
        values
            .iter()
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
                        Some(generate_fake_value_with_rng(faker_type, rng))
                    }
                    None if retain_if_empty => None,
                    _ => unreachable!("nondeterministic branch, not under test"),
                }
            })
            .collect()
    }

    #[test]
    fn transform_with_faker_matches_per_row_reference() {
        // Duplicates must map to the same value; empties and Nones follow `retain_if_empty`.
        let values = ["alice", "", "bob", "alice", "", "carol", "bob"];

        for retain_if_empty in [false, true] {
            // `Md5` (UUIDv4, no rng) and `None` without retain (thread rng) are
            // nondeterministic by design and therefore excluded.
            let faker_types = [
                FakerType::FirstName,
                FakerType::LastName,
                FakerType::Name,
                FakerType::CompanyName,
                FakerType::Email,
                FakerType::PhoneNumber,
                FakerType::Address,
            ];
            let mut column: Vec<Option<&str>> = values.iter().copied().map(Some).collect();
            if retain_if_empty {
                column.extend([None, None]);
            }
            let df =
                DataFrame::new(column.len(), vec![Series::new("a".into(), column).into()]).unwrap();

            for faker_type in faker_types {
                let mut rng = StdRng::seed_from_u64(42);
                let expected = reference(
                    df.column("a").unwrap().str().unwrap(),
                    &mut rng,
                    &faker_type,
                    retain_if_empty,
                );

                let mut rng = StdRng::seed_from_u64(42);
                let output =
                    Faker.transform_with_faker(&df, "a", &mut rng, faker_type, retain_if_empty);
                let actual: Vec<Option<String>> = output[0]
                    .series
                    .str()
                    .unwrap()
                    .iter()
                    .map(|v| v.map(str::to_string))
                    .collect();

                assert_eq!(actual, expected);
                // The caller's rng must come back untouched: the operator threads
                // the same rng through every transformator of a table.
                assert_eq!(rng.next_u64(), StdRng::seed_from_u64(42).next_u64());
            }
        }
    }
}
