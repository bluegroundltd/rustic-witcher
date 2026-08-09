use std::time::Instant;

use anyhow::Result;
use async_trait::async_trait;
use aws_sdk_s3::Client as S3Client;
use aws_sdk_s3::primitives::ByteStream;
use dms_cdc_operator::dataframe::dataframe_ops::CreateDataframePayload;
use dms_cdc_operator::dataframe::dataframe_ops::DataframeOperator;
use polars::polars_utils::pl_path::PlRefPath;
use polars::prelude::{DataFrame, Expr, LazyFrame, ParquetWriter, ScanArgsParquet};
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

    /// Lazily scans a Parquet file directly from S3, pushing the optional row
    /// limit and predicate into the read so only the needed row groups/columns
    /// are fetched and decoded.
    async fn scan_parquet_from_s3(
        &self,
        bucket: &str,
        key: &str,
        n_rows: Option<usize>,
        predicate: Option<Expr>,
    ) -> Result<DataFrame> {
        let cloud_options = self.build_cloud_options().await?;
        let path = format!("s3://{bucket}/{key}");

        let args = ScanArgsParquet {
            n_rows,
            cloud_options: Some(cloud_options),
            // Keys are concrete object paths, never glob patterns; disabling glob
            // avoids an extra LIST round-trip and mis-parsing of `?`/`*`/`[` in keys.
            glob: false,
            ..Default::default()
        };

        // polars 0.54 takes a `PlRefPath`; the `s3://…` scheme is detected from the string.
        let mut lazy = LazyFrame::scan_parquet(PlRefPath::new(path.as_str()), args)
            .map_err(|e| anyhow::anyhow!("failed to scan parquet {path}: {e}"))?;

        if let Some(predicate) = predicate {
            lazy = lazy.filter(predicate);
        }

        // `collect()` performs the cloud range reads and decode. It drives async
        // I/O on Polars' own runtime via `block_on`; calling it on a Tokio worker
        // thread risks a nested-runtime panic and would pin that worker for the
        // whole decode. Offload to a blocking thread so the async runtime stays
        // free and the CPU-heavy decode runs off the I/O reactor.
        let df = tokio::task::spawn_blocking(move || lazy.collect())
            .await
            .map_err(|e| anyhow::anyhow!("parquet scan task panicked: {e}"))?
            .map_err(|e| anyhow::anyhow!("failed to collect scanned parquet {path}: {e}"))?;

        Ok(df)
    }

    /// Builds Polars [`CloudOptions`] whose credentials are resolved through the
    /// *same* aws-sdk credential chain used for every other S3 call (env vars,
    /// IRSA web-identity, IMDS, SSO, ...). This guarantees the lazy scan
    /// authenticates identically to `get_object`, rather than relying on
    /// object_store's separate (and less complete) credential discovery.
    async fn build_cloud_options(&self) -> Result<polars::io::cloud::CloudOptions> {
        use std::future::Future;
        use std::pin::Pin;
        use std::sync::Arc;

        use aws_config::BehaviorVersion;
        use aws_sdk_s3::config::ProvideCredentials;
        use polars::io::cloud::CloudOptions;
        use polars::io::cloud::credential_provider::{
            AwsCredential, ObjectStoreCredential, PlCredentialProvider,
        };
        use polars::prelude::PolarsResult;

        // NOTE: `aws_sdk_s3::Config::credentials_provider()` is deprecated and
        // always returns `None`, so we resolve credentials from a freshly-loaded
        // default `SdkConfig`. This runs the same provider chain the S3 client was
        // built from (env vars / IRSA web-identity / IMDS / SSO).
        let sdk_config = aws_config::load_defaults(BehaviorVersion::latest()).await;

        let creds = sdk_config
            .credentials_provider()
            .ok_or_else(|| anyhow::anyhow!("no AWS credentials provider available"))?
            .provide_credentials()
            .await
            .map_err(|e| anyhow::anyhow!("failed to resolve AWS credentials: {e}"))?;

        // Seconds since UNIX_EPOCH; `u64::MAX` means "never expires" to Polars'
        // credential cache. We rebuild options once per Parquet file, so static
        // credentials still get re-resolved regularly across a run.
        let expiry_secs = creds
            .expiry()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_secs())
            .unwrap_or(u64::MAX);

        let aws_credential = Arc::new(AwsCredential {
            key_id: creds.access_key_id().to_string(),
            secret_key: creds.secret_access_key().to_string(),
            token: creds.session_token().map(str::to_string),
        });

        // The provider closure must return a `Send + Sync` future. We resolved the
        // credentials above, so we return an already-ready future cloning the cached
        // credential — no async work inside, sidestepping the `Sync` future bound.
        let credential_provider = PlCredentialProvider::from_func(move || {
            let aws_credential = aws_credential.clone();
            let fut: Pin<
                Box<
                    dyn Future<Output = PolarsResult<(ObjectStoreCredential, u64)>>
                        + Send
                        + Sync,
                >,
            > = Box::pin(std::future::ready(Ok((
                ObjectStoreCredential::Aws(aws_credential),
                expiry_secs,
            ))));
            fut
        });

        let mut options =
            CloudOptions::default().with_credential_provider(Some(credential_provider));

        if let Some(region) = sdk_config.region() {
            use polars::io::cloud::AmazonS3ConfigKey;
            options = options.with_aws([(AmazonS3ConfigKey::Region, region.to_string())]);
        }

        Ok(options)
    }
}

/// Translates a configured [`FilterType`] into a Polars predicate [`Expr`] that
/// can be pushed into the Parquet scan. Returns `None` for `NoFilter`.
fn build_filter_predicate(filter: &FilterType) -> Option<Expr> {
    use polars::prelude::*;

    let expr = match filter {
        FilterType::Contains { column, value } => {
            col(column.as_str()).str().contains_literal(lit(value.as_str()))
        }
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
            .and(col(column.as_str()).str().ends_with(lit(end_value.as_str()))),
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

        // Lazily scan the Parquet file directly from S3. Projection, predicate and
        // slice pushdown mean we transfer and decode only the row groups/columns we
        // actually need, instead of buffering the whole object into memory first
        // and filtering afterwards.
        let scan_start = Instant::now();
        let df = self
            .scan_parquet_from_s3(&payload.bucket_name, &payload.key, n_rows, predicate)
            .await?;
        info!(
            "{table} parquet file scanned from S3! Time taken: {duration}",
            table = &payload.table_name,
            duration = beautify_duration(scan_start.elapsed()),
        );

        // Null-byte sanitisation now runs *after* the pushed-down filter (it used
        // to run before). Filter columns are keys/types/timestamps that do not
        // carry embedded null bytes, so this reordering is behaviour-preserving in
        // practice while letting the predicate skip row groups during the read.
        let df = if table_config
            .and_then(|c| c.sanitize_null_bytes)
            .unwrap_or(false)
        {
            info!("Sanitizing null bytes for table: {}", &payload.table_name);
            sanitize_null_bytes(df)?
        } else {
            df
        };

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
            .columns()
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
                // `transform` hands back owned `TransformatorOutput`s, so we move the
                // produced `Series` straight into the DataFrame instead of cloning it.
                // `with_column` replaces the existing column of the same name in place.
                for transformator_output in transformator.transform(&df, rng) {
                    info!("Transforming column: {}", transformator_output.column_name);

                    let start = Instant::now();
                    _ = df.with_column(polars::prelude::Column::from(transformator_output.series));

                    info!(
                        "Column transformed! Time taken: {}",
                        beautify_duration(start.elapsed())
                    );
                }
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

// Nullify cells in all String columns that contain an embedded null byte (\x00).
// Stripping \x00 is insufficient when the column holds JSON — the source data is
// truncated at the null byte, leaving invalid JSON that PostgreSQL rejects.
// Setting the whole cell to NULL is safer for corrupted values.
fn sanitize_null_bytes(df: DataFrame) -> Result<DataFrame> {
    use polars::prelude::*;

    let string_col_names: Vec<String> = df
        .columns()
        .iter()
        .filter(|s| matches!(s.dtype(), DataType::String))
        .map(|s| s.name().to_string())
        .collect();

    if string_col_names.is_empty() {
        return Ok(df);
    }

    // Vectorised, columnar replacement: any String cell that contains an embedded
    // null byte is set to NULL. This stays entirely inside Polars' expression
    // engine — no per-row `Vec<Option<String>>` materialisation or per-cell heap
    // allocation — and the engine parallelises the columns for us.
    let exprs: Vec<Expr> = string_col_names
        .iter()
        .map(|name| {
            when(col(name.as_str()).str().contains_literal(lit("\x00")))
                .then(lit(NULL))
                .otherwise(col(name.as_str()))
                .alias(name.as_str())
        })
        .collect();

    Ok(df.lazy().with_columns(exprs).collect()?)
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
