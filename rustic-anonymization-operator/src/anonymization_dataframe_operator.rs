use std::time::Instant;

use anyhow::Result;
use async_trait::async_trait;
use aws_sdk_s3::Client as S3Client;
use aws_sdk_s3::primitives::ByteStream;
use dms_cdc_operator::dataframe::dataframe_ops::CreateDataframePayload;
use dms_cdc_operator::dataframe::dataframe_ops::DataframeOperator;
use polars::polars_utils::pl_path::PlRefPath;
use polars::prelude::{
    DataFrame, Expr, LazyFrame, ParquetWriter, PolarsResult, ScanArgsParquet, Schema,
};
use rand::{SeedableRng, rngs::StdRng};
use rustic_anonymization_config::config_structs::anonymization_config::AnonymizationConfig;
use rustic_anonymization_config::config_structs::filter_type_struct::FilterType;
use rustic_duration::beautify_duration;
use rustic_transformator::transformator_type::TransformatorType;
use rustic_whole_table_transformator::whole_table_transformator::WholeTableTransformator;
use tracing::error;
use tracing::info;

pub struct AnonymizationDataFrameOperator<'a> {
    s3_client: &'a S3Client,
}

impl<'a> AnonymizationDataFrameOperator<'a> {
    pub fn new(s3_client: &'a S3Client) -> Self {
        Self { s3_client }
    }

    /// Lazily scans a Parquet file directly from S3 as one plan:
    /// scan(n_rows) -> null-byte sanitisation -> filter -> collect. The row
    /// limit and the predicate are pushed into the read so only the needed row
    /// groups/columns are fetched and decoded.
    async fn scan_parquet_from_s3(
        bucket: &str,
        key: &str,
        n_rows: Option<usize>,
        sanitize_null_bytes: bool,
        predicate: Option<Expr>,
    ) -> Result<DataFrame> {
        // Memoized process-wide: the same credential chain, region and endpoint
        // as the S3 client, and a single Polars object store for the whole run.
        let cloud_options = rustic_s3_config::polars_cloud_options().await;
        let path = format!("s3://{bucket}/{key}");
        let scan_path = path.clone();

        let args = ScanArgsParquet {
            n_rows,
            cloud_options: Some(cloud_options),
            // Keys are concrete object paths, never glob patterns; disabling glob
            // avoids an extra LIST round-trip and mis-parsing of `?`/`*`/`[` in keys.
            glob: false,
            ..Default::default()
        };

        // `collect_schema()` and `collect()` perform the cloud reads and decode.
        // They drive async I/O on Polars' own runtime via `block_on`; calling
        // them on a Tokio worker thread risks a nested-runtime panic and would
        // pin that worker for the whole decode. Build and run the whole plan on
        // a blocking thread so the async runtime stays free and the CPU-heavy
        // decode runs off the I/O reactor.
        let df = tokio::task::spawn_blocking(move || -> PolarsResult<DataFrame> {
            // polars 0.54 takes a `PlRefPath`; the `s3://…` scheme is detected from the string.
            let mut lazy = LazyFrame::scan_parquet(PlRefPath::new(scan_path.as_str()), args)?;

            if sanitize_null_bytes {
                let exprs = null_byte_sanitize_exprs(lazy.collect_schema()?.as_ref());
                if !exprs.is_empty() {
                    lazy = lazy.with_columns(exprs);
                }
            }

            if let Some(predicate) = predicate {
                lazy = lazy.filter(predicate);
            }

            lazy.collect()
        })
        .await
        .map_err(|e| anyhow::anyhow!("parquet scan task panicked: {e}"))?
        .map_err(|e| anyhow::anyhow!("failed to scan parquet {path}: {e}"))?;

        Ok(df)
    }
}

/// Translates a configured [`FilterType`] into a Polars predicate [`Expr`] that
/// can be pushed into the Parquet scan. Returns `None` for `NoFilter`.
fn build_filter_predicate(filter: &FilterType) -> Option<Expr> {
    use polars::prelude::*;

    let expr = match filter {
        FilterType::Contains { column, value } => col(column.as_str())
            .str()
            .contains_literal(lit(value.as_str())),
        FilterType::StartsWith { column, value } => {
            col(column.as_str()).str().starts_with(lit(value.as_str()))
        }
        FilterType::EndsWith { column, value } => {
            col(column.as_str()).str().ends_with(lit(value.as_str()))
        }
        FilterType::StartsAndEndsWith {
            column,
            start_value,
            end_value,
        } => col(column.as_str())
            .str()
            .starts_with(lit(start_value.as_str()))
            .and(
                col(column.as_str())
                    .str()
                    .ends_with(lit(end_value.as_str())),
            ),
        FilterType::Equals { column, value } => col(column.as_str()).eq(lit(value.as_str())),
        FilterType::AnyOfInt { column, values } => {
            let excluded = Series::new("excluded".into(), values.clone());
            col(column.as_str()).is_in(lit(excluded), true).not()
        }
        FilterType::AnyOfString { column, values } => {
            let excluded = Series::new("excluded".into(), values.clone());
            col(column.as_str()).is_in(lit(excluded), true).not()
        }
        FilterType::NoFilter => return None,
    };

    Some(expr)
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
        // `load_config_for` is now memoized process-wide, so this is a cheap
        // Arc clone after the first call instead of a disk read + TOML parse
        // on every Parquet file.
        let config = AnonymizationConfig::load_config_for(
            payload.database_name.as_str(),
            payload.schema_name.as_str(),
        );
        let table_config = config.fetch_table_config(&payload.table_name);

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

        // Push record-reduction down into the scan: for the first LOAD file we
        // read at most `keep_num_of_records` rows straight out of the Parquet
        // file (slice pushdown) instead of materialising the whole DataFrame and
        // slicing afterwards.
        let n_rows = if record_reduction_is_enabled && is_first_load_file {
            table_config.and_then(|c| c.keep_num_of_records)
        } else {
            None
        };

        // Translate the configured filter (if any) into a Polars predicate that
        // is pushed into the Parquet read. Row groups whose statistics prove they
        // cannot match are skipped — never fetched from S3, never decoded.
        let predicate = table_config
            .and_then(|c| c.filter_type.as_ref())
            .and_then(build_filter_predicate);

        let sanitize_null_bytes = table_config
            .and_then(|c| c.sanitize_null_bytes)
            .unwrap_or(false);
        if sanitize_null_bytes {
            info!("Sanitizing null bytes for table: {}", &payload.table_name);
        }

        // Lazily scan the Parquet file directly from S3. Projection, predicate and
        // slice pushdown mean we transfer and decode only the row groups/columns we
        // actually need, instead of buffering the whole object into memory first
        // and filtering afterwards.
        //
        // Keep this the first `.await` of this method: `cdc_operator` overlaps the
        // next file's scan with the current insert via `buffered(2)`, which only
        // works if the first poll reaches the scan's `spawn_blocking`.
        let scan_start = Instant::now();
        let df = Self::scan_parquet_from_s3(
            &payload.bucket_name,
            &payload.key,
            n_rows,
            sanitize_null_bytes,
            predicate,
        )
        .await?;
        info!(
            "{table} parquet file scanned from S3! Time taken: {duration}",
            table = &payload.table_name,
            duration = beautify_duration(scan_start.elapsed()),
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

        let df_get_column_names_start = Instant::now();
        let column_names = df
            .columns()
            .iter()
            .map(|s| s.name().to_string())
            .collect::<Vec<String>>();
        let df_get_column_names_duration = beautify_duration(df_get_column_names_start.elapsed());
        info!("Get column names duration: {df_get_column_names_duration}");

        let rng_seed = rng_seed();

        info!("Will anonymize with SEED: {rng_seed}!");

        // Start anonymizing the Dataframe. Faker generation is pure CPU, so it
        // runs on a blocking thread instead of pinning a shared async worker.
        let anonymization_start = Instant::now();
        let df = tokio::task::spawn_blocking(move || {
            let mut df = df;
            let rng = &mut StdRng::seed_from_u64(rng_seed);

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
                    // `transform` hands back owned `TransformatorOutput`s, so we move the
                    // produced `Series` straight into the DataFrame instead of cloning it.
                    // `with_column` replaces the existing column of the same name in place.
                    for transformator_output in transformator.transform(&df, rng) {
                        info!("Transforming column: {}", transformator_output.column_name);

                        let start = Instant::now();
                        _ = df.with_column(polars::prelude::Column::from(
                            transformator_output.series,
                        ));

                        info!(
                            "Column transformed! Time taken: {}",
                            beautify_duration(start.elapsed())
                        );
                    }
                });

            df
        })
        .await
        .map_err(|e| anyhow::anyhow!("anonymization task panicked: {e}"))?;
        info!(
            "Anonymization done! Time taken: {}",
            beautify_duration(anonymization_start.elapsed())
        );

        if !should_upload_anonymized_files() {
            return Ok(Some(df));
        }

        // Upload anonymized Parquet files to anonymized S3 bucket.
        let df = upload_parquet_file(self.s3_client, df, payload.key.as_str()).await;

        Ok(Some(df))
    }
}

// Nullify cells in all String columns that contain an embedded null byte (\x00).
// Stripping \x00 is insufficient when the column holds JSON — the source data is
// truncated at the null byte, leaving invalid JSON that PostgreSQL rejects.
// Setting the whole cell to NULL is safer for corrupted values.
//
// Vectorised, columnar replacement expressed on the scan's schema so it fuses
// into the lazy plan: no per-row `Vec<Option<String>>` materialisation or
// per-cell heap allocation, and the engine parallelises the columns for us.
fn null_byte_sanitize_exprs(schema: &Schema) -> Vec<Expr> {
    use polars::prelude::*;

    schema
        .iter()
        .filter(|(_, dtype)| matches!(dtype, DataType::String))
        .map(|(name, _)| {
            when(col(name.as_str()).str().contains_literal(lit("\x00")))
                .then(lit(NULL))
                .otherwise(col(name.as_str()))
                .alias(name.as_str())
        })
        .collect()
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

// Upload anonymized Parquet files to anonymized S3 bucket. Takes and hands back
// the DataFrame so the CPU-bound Parquet encode can run on a blocking thread.
async fn upload_parquet_file(
    s3_client: &S3Client,
    df: DataFrame,
    parquet_s3_key: &str,
) -> DataFrame {
    // Upload anonymized Parquet files to anonymized S3 bucket.
    let df_to_parquet_start = Instant::now();

    let (df, parquet_write_result) = tokio::task::spawn_blocking(move || {
        let mut df = df;
        let mut buf = vec![];
        let result = ParquetWriter::new(&mut buf)
            .with_row_group_size(Some(10000))
            .set_parallel(true)
            .finish(&mut df);
        (df, result.map(|_| buf))
    })
    .await
    .expect("parquet encode task panicked");

    let buf = match parquet_write_result {
        Ok(buf) => buf,
        Err(e) => {
            error!("Error writing parquet file: {:?}", e);
            return df;
        }
    };

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

    df
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
