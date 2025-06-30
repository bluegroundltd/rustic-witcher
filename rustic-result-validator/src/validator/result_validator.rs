use bon::Builder;
use colored::Colorize;
use deadpool_postgres::{GenericClient, Pool};
use tracing::info;

use crate::config_structs::{root_struct::Validations, value_check_type_struct::ValueCheckType};

/// The `ResultValidator` struct is responsible for validating results from a database.
///
/// It contains the following fields:
/// - `database_name`: The name of the database to validate.
/// - `schema_name`: The name of the schema to validate.
/// - `target_db_pool`: The connection pool for the target database.
/// - `validations`: The validations to perform.
///
/// # Example
///
/// ```
/// use deadpool_postgres::Pool;
/// use crate::config_structs::{root_struct::Validations, value_check_type_struct::ValueCheckType};
///
/// let target_db_pool: Pool = // create a connection pool for the target database
/// let validations: Validations = // define the validations to perform
///
/// let result_validator = ResultValidator::new(
///     "database_name",
///     "schema_name",
///     target_db_pool,
///     validations,
/// );
///
/// result_validator.validate().await;
/// ```
#[derive(Builder)]
pub struct ResultValidator {
    database_name: String,
    schema_name: String,
    target_db_pool: Pool,
    validations: Validations,
}

impl ResultValidator {
    /// Validates the results from the database.
    ///
    /// This method performs the specified validations on the target database.
    ///
    /// # Example
    ///
    /// ```
    /// use deadpool_postgres::Pool;
    /// use crate::config_structs::{root_struct::Validations, value_check_type_struct::ValueCheckType};
    ///
    /// let target_db_pool: Pool = // create a connection pool for the target database
    /// let validations: Validations = // define the validations to perform
    ///
    /// let result_validator = ResultValidator::new(
    ///     "database_name",
    ///     "schema_name",
    ///     target_db_pool,
    ///     validations,
    /// );
    ///
    /// result_validator.validate().await;
    /// ```
    pub async fn validate(&self) {
        // Check if we will skip validations
        let skip_validations = should_skip_validations();

        if skip_validations {
            info!("Skipping validations");
            return;
        }

        let client = self.target_db_pool.get().await.unwrap();
        let validations = &self.validations.validations;

        for ele in validations {
            let result = client
                .query(ele.query.as_str(), &[])
                .await
                .expect("Failed to validate");
            let value_check_type = ele.value_check_type.clone();

            info!(
                "{}",
                format!(
                    "Validating table: {} in schema: {} for database: {}",
                    ele.table, self.schema_name, self.database_name
                )
                .bold()
                .blue()
            );

            match value_check_type {
                ValueCheckType::Equals { ref value } => {
                    let all_equal = result
                        .iter()
                        .map(|row| row.try_get(ele.column_to_check.as_str()).unwrap())
                        .all(|row_value: String| {
                            let is_equal = &row_value == value;
                            if is_equal {
                                info!(
                                    "Passed! Desired value: {}, Equals: {}",
                                    value.bold().green(),
                                    row_value.bold().green()
                                );
                            }
                            is_equal
                        });

                    if !all_equal {
                        panic!(
                            "Validation failed! Not all rows had the expected value: {value}"
                        );
                    }
                }
                ValueCheckType::Contains { ref value } => {
                    let all_contain = result
                        .iter()
                        .map(|row| row.try_get(ele.column_to_check.as_str()).unwrap())
                        .all(|row_value: String| {
                            let contains = row_value.contains(value);
                            if contains {
                                info!(
                                    "Passed! Desired part of value: {}, Contains: {}",
                                    value.bold().green(),
                                    row_value.bold().green()
                                );
                            }
                            contains
                        });

                    if !all_contain {
                        panic!(
                            "Validation failed: Not all rows contained the expected value: {value}"
                        );
                    }
                }
            }
        }
    }
}

// Control if we will skip the validations
fn should_skip_validations() -> bool {
    std::env::var("SKIP_VALIDATIONS")
        .unwrap_or_else(|_| "false".to_string())
        .parse()
        .unwrap()
}
