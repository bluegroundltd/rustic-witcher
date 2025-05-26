pub mod fake_email_with_id_prefix_transformator;
pub mod fake_multi_email_transformator;
pub mod fake_phone_transformator;

#[cfg(test)]
pub mod tests;

use polars::prelude::*;
use rand::rngs::StdRng;
use rustic_faker_types::FakerType;
use rustic_transformator::transformator::Transformator;
use rustic_transformator::transformator_output::TransformatorOutput;
use rustic_transformator::transformator_type::TransformatorType;

macro_rules! create_faker_transformator {
    ($struct_name:ident, $faker_type:expr) => {
        pub struct $struct_name {
            pub column_name: String,
            pub retain_if_empty: bool,
        }

        impl $struct_name {
            pub fn new(column_name: impl Into<String>, retain_if_empty: bool) -> Self {
                Self {
                    column_name: column_name.into(),
                    retain_if_empty,
                }
            }
        }

        impl Transformator for $struct_name {
            fn transform(&self, input: &DataFrame, rng: &mut StdRng) -> Vec<TransformatorOutput> {
                self.transform_with_faker(
                    input,
                    &self.column_name,
                    rng,
                    $faker_type,
                    self.retain_if_empty,
                )
            }

            fn transformator_type(&self) -> TransformatorType {
                TransformatorType::MultiColumn
            }
        }
    };
}

create_faker_transformator!(FakeNameTransformator, FakerType::Name);
create_faker_transformator!(FakeAddressTransformator, FakerType::Address);
create_faker_transformator!(FakeCompanyNameTransformator, FakerType::CompanyName);
create_faker_transformator!(FakeEmailTransformator, FakerType::Email);
create_faker_transformator!(FakeFirstnameTransformator, FakerType::FirstName);
create_faker_transformator!(FakeLastNameTransformator, FakerType::LastName);
create_faker_transformator!(FakeMd5Transformator, FakerType::Md5);
