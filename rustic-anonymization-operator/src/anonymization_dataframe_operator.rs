use std::time::Instant;

use anyhow::Result;
use async_trait::async_trait;
use aws_sdk_s3::Client as S3Client;
use aws_sdk_s3::primitives::ByteStream;
use dms_cdc_operator::dataframe::dataframe_ops::CreateDataframePayload;
use dms_cdc_operator::dataframe::dataframe_ops::DataframeOperator;
use polars::prelude::IntoLazy;
use polars::prelude::col;
use polars::prelude::lit;
use polars::prelude::{DataFrame, ParquetWriter};
use polars::{
    io::SerReader as _,
    prelude::{ParallelStrategy, ParquetReader},
};
use rand::{SeedableRng, rngs::StdRng};
use rustic_anonymization_config::config_structs::anonymization_config::AnonymizationConfig;
use rustic_anonymization_config::config_structs::filter_type_struct::FilterType;
use rustic_duration::beautify_duration;
use rustic_transformator::transformator_type::TransformatorType;
use rustic_whole_table_transformator::whole_table_transformator::WholeTableTransformator;
use tracing::error;
use tracing::{debug, info};

pub struct AnonymizationDataFrameOperator<'a> {
    s3_client: &'a S3Client,
}

impl<'a> AnonymizationDataFrameOperator<'a> {
    pub fn new(s3_client: &'a S3Client) -> Self {
        Self { s3_client }
    }
}

#[async_trait]
/// Implements the `DataframeOperator` trait for the `AnonymizedDataFrameOperator` struct.
/// This struct provides methods for creating a dataframe from a Parquet file and applying anonymization transformations.
impl DataframeOperator for AnonymizationDataFrameOperator<'_> {
    /// Creates a dataframe from a Parquet file.
    ///
    /// # Arguments
    ///
    /// * `payload` - The payload containing information about the Parquet file to be loaded.
    ///
    /// # Returns
    ///
    /// Returns a `Result` containing an optional `DataFrame`.
    /// If the Parquet file is not to be loaded (based on certain conditions), `Ok(None)` is returned.
    /// Otherwise, `Ok(Some(df))` is returned, where `df` is the loaded dataframe.
    async fn create_dataframe_from_parquet_file(
        &self,
        payload: &CreateDataframePayload,
    ) -> Result<Option<DataFrame>> {
        let table_config: AnonymizationConfig = AnonymizationConfig::load_config_for(
            payload.database_name.as_str(),
            payload.schema_name.as_str(),
        );
        let table_config = table_config.fetch_table_config(&payload.table_name);

        // Check if we are operating on the first load file.
        // If we do, we need to check if there is a [keep_num_of_records]
        // option, for this table.
        // If there is we need to handle it accordingly and skip the
        // anonymization.
        //
        // TODO: Side note (for future us): Check what happens if the number
        // of rows in the `.parquet` file is not enough based on the
        // [keep_num_of_records] option.
        let is_first_load_file = payload.key.contains("LOAD00000001");
        let has_num_of_records = match table_config {
            Some(table_config) => table_config.keep_num_of_records.is_some(),
            None => false,
        };

        // Check if we allow record reduction.
        // Controlled by [RECORD_REDUCTION_ENABLED] env variable.
        let record_reduction_is_enabled = is_record_reduction_enabled();

        // If it is a LOAD file, and it is not the first one, we should not load the file
        if !is_first_load_file && has_num_of_records && record_reduction_is_enabled {
            return Ok(None);
        }

        // Download the relevant `.parquet` file from S3.
        info!("Reading parquet file from S3: {}", payload.key);
        let df_download_start = Instant::now();
        let mut object = self
            .s3_client
            .get_object()
            .bucket(&payload.bucket_name)
            .key(&payload.key)
            .send()
            .await?;
        let df_download_duration = beautify_duration(df_download_start.elapsed());
        info!(
            "{} Parquet file downloaded! Time taken: {df_download_duration}",
            payload.key,
            df_download_duration = df_download_duration,
        );

        let mut file_vec = Vec::new();
        while let Some(bytes) = object.body.try_next().await? {
            debug!("Read {} bytes", bytes.len());
            file_vec.extend_from_slice(&bytes);
        }

        // Prepare to load the `.parquet` file.
        let df_load_start = Instant::now();
        let cursor = std::io::Cursor::new(file_vec);
        let reader = ParquetReader::new(cursor);
        let mut df = reader;

        // If we are operating on the first `LOAD` file,
        // we check for the [keep_num_of_records] option,
        // in order to avoid loading the full Dataframe in memory.
        let df = if record_reduction_is_enabled {
            if let Some(table_config) = table_config {
                if is_first_load_file {
                    let slice_size = table_config
                        .keep_num_of_records
                        .unwrap_or(df.num_rows().unwrap());
                    df.with_slice(Some((0, slice_size)))
                        .read_parallel(ParallelStrategy::Auto)
                        .finish()
                        .unwrap()
                } else {
                    df.read_parallel(ParallelStrategy::Auto).finish().unwrap()
                }
            } else {
                df.read_parallel(ParallelStrategy::Auto).finish().unwrap()
            }
        } else {
            df.read_parallel(ParallelStrategy::Auto).finish().unwrap()
        };

        let df_load_duration = beautify_duration(df_load_start.elapsed());
        info!(
            "{table} parquet file loaded! Time taken: {df_load_duration}",
            table = &payload.table_name,
        );

        // Filter [DataFrame] based on the filter type,
        // if any was supplied.
        let df_filter_start = Instant::now();
        let df = match table_config {
            Some(table_config) => {
                if let Some(filter) = &table_config.filter_type {
                    match filter {
                        FilterType::Contains { column, value } => {
                            let filter_expr = col(column.as_str())
                                .str()
                                .contains_literal(lit(value.as_str()));
                            df.lazy().filter(filter_expr).collect()?
                        }
                        FilterType::StartsWith { column, value } => {
                            let filter_expr =
                                col(column.as_str()).str().starts_with(lit(value.as_str()));
                            df.lazy().filter(filter_expr).collect()?
                        }
                        FilterType::EndsWith { column, value } => {
                            let filter_expr =
                                col(column.as_str()).str().ends_with(lit(value.as_str()));
                            df.lazy().filter(filter_expr).collect()?
                        }
                        FilterType::NoFilter => df,
                    }
                } else {
                    df
                }
            }
            None => df,
        };
        let df_filter_duration = beautify_duration(df_filter_start.elapsed());
        info!(
            "{table} parquet file filtered! Time taken: {df_filter_duration}",
            table = &payload.table_name,
        );

        // If there are no `Transformator`s we can return the already
        // read Dataframe.
        let transformators = if let Some(table_config) = table_config {
            table_config.build_transformators(whole_table_transformator())
        } else {
            if should_upload_anonymized_files() {
                copy_parquet_file_to_anonymized_bucket(
                    self.s3_client,
                    payload.bucket_name.as_str(),
                    payload.key.as_str(),
                )
                .await;
            }
            return Ok(Some(df));
        };

        let mut df = df;
        let df_get_column_names_start = Instant::now();
        let column_names = df
            .get_columns()
            .iter()
            .map(|s| s.name().to_string())
            .collect::<Vec<String>>();
        let df_get_column_names_duration = beautify_duration(df_get_column_names_start.elapsed());
        info!("Get column names duration: {df_get_column_names_duration}");

        let df_to_owned_start = Instant::now();
        let df_to_owned_duration = beautify_duration(df_to_owned_start.elapsed());
        info!("To owned duration: {df_to_owned_duration}");

        let rng_seed = rng_seed();

        info!("Will anonymize with SEED: {rng_seed}!");

        let rng = &mut StdRng::seed_from_u64(rng_seed);

        // Start anonymizing the Dataframe.
        let anonymization_start = Instant::now();
        transformators
            .iter()
            .filter(|transformator| match &transformator.transformator_type() {
                // In case we have a `NoOpTransformator` we just skip it from the
                // next operations.
                TransformatorType::NoOp => false,
                // Single column `Transformators` need to be checked against the available
                // columns on the Dataframe.
                TransformatorType::SingleColumn { column_name } => {
                    column_names.contains(column_name)
                }
                _ => true,
            })
            .for_each(|transformator| {
                transformator
                    .transform(&df, rng)
                    .iter()
                    .for_each(|transformator_output| {
                        info!("Transforming column: {}", transformator_output.column_name);

                        let start = Instant::now();
                        _ = df.apply(transformator_output.column_name.as_str(), |_| {
                            transformator_output.series.clone()
                        });

                        info!(
                            "Column transformed! Time taken: {}",
                            beautify_duration(start.elapsed())
                        );
                    });
            });
        info!(
            "Anonymization done! Time taken: {}",
            beautify_duration(anonymization_start.elapsed())
        );

        if !should_upload_anonymized_files() {
            return Ok(Some(df));
        }

        // Upload anonymized Parquet files to anonymized S3 bucket.
        upload_parquet_file(self.s3_client, &mut df, payload.key.as_str()).await;

        Ok(Some(df))
    }
}

// Copy Parquet file to anonymized S3 bucket.
async fn copy_parquet_file_to_anonymized_bucket(
    s3_client: &S3Client,
    parquet_s3_bucket: &str,
    parquet_s3_key: &str,
) {
    // Upload anonymized Parquet files to anonymized S3 bucket.
    let parquet_copy_start = Instant::now();

    let source_bucket_and_object = format!("{parquet_s3_bucket}/{parquet_s3_key}");
    let destination_bucket = anonymized_bucket();

    _ = s3_client
        .copy_object()
        .copy_source(source_bucket_and_object)
        .bucket(&destination_bucket)
        .key(parquet_s3_key)
        .send()
        .await;

    let parquet_copy_duration = beautify_duration(parquet_copy_start.elapsed());
    info!(
        "Parquet file copied from {source_bucket} to {destination_bucket}! Time taken: {parquet_copy_duration}",
        source_bucket = parquet_s3_bucket,
        parquet_copy_duration = parquet_copy_duration,
    );
}

// Upload anonymized Parquet files to anonymized S3 bucket.
async fn upload_parquet_file(s3_client: &S3Client, df: &mut DataFrame, parquet_s3_key: &str) {
    // Upload anonymized Parquet files to anonymized S3 bucket.
    let df_to_parquet_start = Instant::now();

    let mut buf = vec![];
    let parquet_write_result = ParquetWriter::new(&mut buf)
        .with_row_group_size(Some(10000))
        .set_parallel(true)
        .finish(df);

    if let Err(e) = parquet_write_result {
        error!("Error writing parquet file: {:?}", e);
        return;
    }

    let file_stream = ByteStream::from(buf);

    _ = s3_client
        .put_object()
        .bucket(anonymized_bucket())
        .key(parquet_s3_key)
        .body(file_stream)
        .send()
        .await
        .unwrap();

    let df_to_parquet_duration = beautify_duration(df_to_parquet_start.elapsed());
    info!(
        "Dataframe anonymized and saved to S3! Time taken: {df_to_parquet_duration}",
        df_to_parquet_duration = df_to_parquet_duration,
    );
}

// Control the allowance of reduced dataset generation
//
// Note: might come with specific edge cases, refer to the
// comments above.
fn is_record_reduction_enabled() -> bool {
    std::env::var("RECORD_REDUCTION_ENABLED")
        .unwrap_or_else(|_| "false".to_string())
        .parse()
        .unwrap()
}

// Control the RNG seed of the anonymization
fn rng_seed() -> u64 {
    std::env::var("RNG_SEED")
        .unwrap_or_else(|_| "42".to_string())
        .parse()
        .unwrap()
}

// Control the upload of anonymized Parquet files to the anonymized
// bucket
fn should_upload_anonymized_files() -> bool {
    std::env::var("UPLOAD_ANONYMIZED_FILES")
        .unwrap_or_else(|_| "false".to_string())
        .parse()
        .unwrap()
}

// Anonymized Parquet files S3 bucket
fn anonymized_bucket() -> String {
    std::env::var("ANONYMIZED_BUCKET").expect("ANONYMIZED_BUCKET env var not set!")
}

#[cfg(not(feature = "open_source"))]
fn whole_table_transformator() -> impl WholeTableTransformator {
    use rustic_bg_whole_table_transformator::BgWholeTableTransformator;

    BgWholeTableTransformator::new()
}

#[cfg(feature = "open_source")]
fn whole_table_transformator() -> impl WholeTableTransformator {
    use rustic_whole_table_transformator::whole_table_transformator::NoOpWholeTableTransformator;

    NoOpWholeTableTransformator::new()
}
