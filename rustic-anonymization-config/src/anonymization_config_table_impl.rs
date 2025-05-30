use crate::config_structs::table_struct::AnonymizationConfigTable;
use crate::config_structs::table_type_struct::AnonymizationConfigTableType;
use crate::config_structs::transformation_type_struct::AnonymizationTransformationType;

use rustic_base_transformations::nulify_transformator::NullifyTransformator;
use rustic_base_transformations::replace_transformator::ReplaceTransformator;
use rustic_faker_transformations::faker_transformators::fake_email_with_id_prefix_transformator::FakeEmailWithIdPrefixTransformator;
use rustic_faker_transformations::faker_transformators::fake_multi_email_transformator::FakeMultiEmailTransformator;
use rustic_faker_transformations::faker_transformators::fake_phone_transformator::FakePhoneTransformator;
use rustic_faker_transformations::faker_transformators::{
    FakeAddressTransformator, FakeCompanyNameTransformator, FakeEmailTransformator,
    FakeFirstnameTransformator, FakeLastNameTransformator, FakeMd5Transformator,
    FakeNameTransformator,
};
use rustic_transformator::transformator::Transformator;
use rustic_whole_table_transformator::whole_table_transformator::WholeTableTransformator;

/// Implementation of the `AnonymizationConfigTable` struct.
impl AnonymizationConfigTable {
    /// Builds and returns a vector of transformators based on the `AnonymizationConfigTable` instance.
    /// The transformators are created based on the specified anonymization type and column transformations.
    pub fn build_transformators(
        &self,
        whole_table_transformator: impl WholeTableTransformator,
    ) -> Vec<Box<dyn Transformator>> {
        match &self.anonymization_type {
            AnonymizationConfigTableType::Multi {
                column_transformations,
            } => column_transformations
                .iter()
                .map(|column_transformation| {
                    let column_name = column_transformation.column_name.as_str();
                    Self::define_transformation_type(
                        column_name,
                        column_transformation.transformation_type.clone(),
                        column_transformation.retain_if_empty.unwrap_or(false),
                    )
                })
                .collect(),
            AnonymizationConfigTableType::Single { transformation } => {
                vec![whole_table_transformator.transform(transformation.as_str())]
            }
        }
    }

    /// Defines the transformation type based on the specified column name, transformation type, and retain if empty flag.
    /// Returns a box containing the corresponding transformator.
    fn define_transformation_type(
        column_name: &str,
        transformation_type: AnonymizationTransformationType,
        retain_if_empty: bool,
    ) -> Box<dyn Transformator> {
        match transformation_type {
            AnonymizationTransformationType::Replace { replacement_value } => {
                Box::new(ReplaceTransformator::new(column_name, replacement_value))
            }
            AnonymizationTransformationType::Custom { operation_type } => {
                Self::match_transformator(column_name, operation_type.as_str(), retain_if_empty)
            }
            AnonymizationTransformationType::Nullify => {
                Box::new(NullifyTransformator::new(column_name))
            }
        }
    }

    /// Matches the specified operation type and returns the corresponding transformator based on the specified column name and retain if empty flag.
    fn match_transformator(
        column_name: &str,
        operation_type_raw: &str,
        retain_if_empty: bool,
    ) -> Box<dyn Transformator> {
        match operation_type_raw {
            "fake_phone_transformation" => Box::new(
                FakePhoneTransformator::builder()
                    .column_name(column_name.to_string())
                    .build(),
            ),
            "fake_firstname_transformation" => Box::new(FakeFirstnameTransformator::new(
                column_name,
                retain_if_empty,
            )),
            "fake_lastname_transformation" => {
                Box::new(FakeLastNameTransformator::new(column_name, retain_if_empty))
            }
            "fake_name_transformation" => {
                Box::new(FakeNameTransformator::new(column_name, retain_if_empty))
            }
            "fake_email_transformation" => {
                Box::new(FakeEmailTransformator::new(column_name, retain_if_empty))
            }
            "fake_multi_email_transformation" => Box::new(
                FakeMultiEmailTransformator::builder()
                    .column_name(column_name.to_string())
                    .build(),
            ),
            "fake_companyname_transformation" => Box::new(FakeCompanyNameTransformator::new(
                column_name,
                retain_if_empty,
            )),
            "fake_address_transformation" => {
                Box::new(FakeAddressTransformator::new(column_name, retain_if_empty))
            }
            "fake_md5_transformation" => {
                Box::new(FakeMd5Transformator::new(column_name, retain_if_empty))
            }
            "fake_email_with_id_prefix_transformation" => Box::new(
                FakeEmailWithIdPrefixTransformator::builder()
                    .column_name(column_name.to_string())
                    .build(),
            ),
            _ => panic!("Unknown operation type: {operation_type_raw}"),
        }
    }
}
