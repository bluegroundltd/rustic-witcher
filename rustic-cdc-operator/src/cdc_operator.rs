use aws_sdk_s3::Client as S3Client;
use colored::Colorize;
use core::panic;
use deadpool_postgres::{Pool, Runtime, tokio_postgres::NoTls};
use dms_cdc_operator::dataframe::dataframe_ops::DataframeOperator;
use dms_cdc_operator::{
    cdc::snapshot_payload::CDCOperatorSnapshotPayload,
    dataframe::dataframe_ops::CreateDataframePayload,
    postgres::postgres_operator::{
        InsertDataframePayload, PostgresOperator, UpsertDataframePayload,
    },
    s3::s3_operator::{LoadParquetFilesPayload, S3Operator, S3OperatorImpl},
};
use rustic_anonymization_operator::anonymization_dataframe_operator::AnonymizationDataFrameOperator;
use rustic_duration::beautify_duration;
use rustic_target_db::prepare_db_config;
use rustic_target_db::target_db_finalizer::TargetDBFinalizer;
use rustic_target_db::target_db_preparator::TargetDbPreparator;
use std::{env, sync::Arc, time::Instant};
use tracing::info;

pub struct CDCOperator;

impl CDCOperator {
    /// Prepares for a snapshot by taking a pg_dump of the source DB, dropping the schema in the target DB,
    /// restoring the schema in the target DB, creating a super-user for data import, and importing sequences last values.
    ///
    /// # Arguments
    ///
    /// * `cdc_operator_snapshot_payload` - The payload containing the necessary information for snapshotting.
    pub async fn prepare_for_snapshot(cdc_operator_snapshot_payload: &CDCOperatorSnapshotPayload) {
        // Prepare source DB configuration
        let source_cfg = prepare_db_config(cdc_operator_snapshot_payload.source_postgres_url());
        let source_pool = source_cfg
            .create_pool(Some(Runtime::Tokio1), NoTls)
            .unwrap();

        // Prepare target DB configuration
        let target_cfg = prepare_db_config(cdc_operator_snapshot_payload.target_postgres_url());
        let target_pool = target_cfg
            .create_pool(Some(Runtime::Tokio1), NoTls)
            .unwrap();

        let target_db_preparator = TargetDbPreparator {
            target_db_pool: target_pool,
            source_db_pool: source_pool,
        };

        info!("{}", "Taking a pg_dump of the source DB".bold().blue());
        target_db_preparator
            .pg_dump_schema(cdc_operator_snapshot_payload)
            .await;

        info!(
            "{}",
            "Dropping the schema in the target DB...".bold().blue()
        );
        target_db_preparator
            .drop_schema(cdc_operator_snapshot_payload)
            .await;

        info!(
            "{}",
            "Restoring the schema in the target DB...".bold().blue()
        );
        target_db_preparator
            .pg_restore_schema(cdc_operator_snapshot_payload)
            .await;

        info!("{}", "Creating super-user for data import...".bold().blue());
        let target_postgres_url = cdc_operator_snapshot_payload.target_postgres_url();

        #[rustfmt::skip]
        let target_superuser_username: &str = target_postgres_url
            .split("://")
            .collect::<Vec<_>>()[1]
            .split(':')
            .collect::<Vec<_>>()[0];

        target_db_preparator
            .create_data_import_user(
                cdc_operator_snapshot_payload.schema_name().as_str(),
                target_superuser_username,
            )
            .await;

        info!("{}", "Fixing sequences ownership...".bold().blue());
        target_db_preparator
            .fix_sequences_ownership(
                cdc_operator_snapshot_payload.database_name().as_str(),
                cdc_operator_snapshot_payload.schema_name().as_str(),
            )
            .await;
    }

    /// Takes a snapshot of the data stored in S3 and replicates them in a target database.
    ///
    /// # Arguments
    ///
    /// * `cdc_operator_snapshot_payload` - The payload containing the necessary information for snapshotting.
    /// * `source_postgres_operator` - The implementation of the PostgresOperator trait for the source database.
    /// * `superuser_postgres_operator` - The implementation of the PostgresOperator trait for the target database.
    /// * `s3_client` - The S3 client for accessing the Parquet files.
    pub async fn snapshot(
        cdc_operator_snapshot_payload: &CDCOperatorSnapshotPayload,
        source_postgres_operator: &(impl PostgresOperator + Sync),
        superuser_postgres_operator: &(impl PostgresOperator + Sync),
        s3_client: &S3Client,
    ) {
        info!("{}", "Starting snapshotting...".bold().blue());

        // Find tables that will be included in the snapshotting operation
        let get_tables_in_schema_start = Instant::now();
        let table_list = source_postgres_operator
            .get_tables_in_schema(
                cdc_operator_snapshot_payload.schema_name().as_str(),
                cdc_operator_snapshot_payload.included_tables().as_slice(),
                cdc_operator_snapshot_payload.excluded_tables().as_slice(),
                &cdc_operator_snapshot_payload.table_mode(),
            )
            .await
            .unwrap();
        let get_tables_in_schema_duration = beautify_duration(get_tables_in_schema_start.elapsed());
        info!(
            "Load tables in schema duration: {}",
            get_tables_in_schema_duration
        );

        // Prepare [Arc]s for usage in multi threaded operations below.
        let cdc_operator_snapshot_payload: Arc<&CDCOperatorSnapshotPayload> =
            Arc::new(cdc_operator_snapshot_payload);
        let client = s3_client.clone();
        let s3_operator = Arc::new(S3OperatorImpl::new(&client));
        let dataframe_operator = Arc::new(AnonymizationDataFrameOperator::new(s3_client));

        let anonymized_tables = table_list
            .iter()
            .map(|table| {
                let payload = cdc_operator_snapshot_payload.clone();
                let s3_operator = s3_operator.clone();
                let dataframe_operator = dataframe_operator.clone();

                async move {
                    let payload = payload.clone();
                    let start = Instant::now();
                    info!(
                        "{}",
                        format!("Running for table: {}", table).bold().magenta()
                    );

                    // Get the table columns
                    info!("{}", "Getting table columns".bold().green());
                    let get_table_columns_start = Instant::now();
                    let source_table_columns: indexmap::IndexMap<String, String> =
                        source_postgres_operator
                            .get_table_columns(payload.schema_name().as_str(), table.as_str())
                            .await
                            .unwrap();
                    let get_table_columns_duration =
                        beautify_duration(get_table_columns_start.elapsed());
                    info!("Get table columns duration: {}", get_table_columns_duration);
                    info!(
                        "Number of columns: {}, Columns: {:?}",
                        source_table_columns.len(),
                        source_table_columns
                    );

                    // Get the primary key for the table
                    info!("{}", "Getting primary key".bold().green());
                    let get_primary_key_start = Instant::now();
                    let primary_key_list = source_postgres_operator
                        .get_primary_key(table.as_str(), payload.schema_name().as_str())
                        .await
                        .unwrap();
                    let get_primary_key_duration =
                        beautify_duration(get_primary_key_start.elapsed());
                    info!("Get primary keys duration: {}", get_primary_key_duration);
                    info!("Primary key(s): {:?}", primary_key_list);

                    // Get the list of Parquet files from S3 that are related to the table
                    info!("{}", "Getting list of Parquet files from S3".bold().green());

                    // Check if mode is DateAware and start_date is not None
                    if payload.mode_is_date_aware() && payload.start_date().is_none() {
                        panic!("start_date is required for DateAware mode");
                    }

                    let load_parquet_files_payload = if payload.mode_is_date_aware() {
                        LoadParquetFilesPayload::DateAware {
                            bucket_name: payload.bucket_name().clone(),
                            s3_prefix: payload.key().clone(),
                            database_name: payload.database_name().clone(),
                            schema_name: payload.schema_name().clone(),
                            table_name: table.to_string(),
                            start_date: payload.start_date().clone().unwrap(),
                            stop_date: payload.stop_date().clone(),
                        }
                    } else if payload.mode_is_full_load_only() {
                        LoadParquetFilesPayload::FullLoadOnly {
                            bucket_name: payload.bucket_name().clone(),
                            s3_prefix: payload.key().clone(),
                            database_name: payload.database_name().clone(),
                            schema_name: payload.schema_name().clone(),
                            table_name: table.to_string(),
                        }
                    } else {
                        LoadParquetFilesPayload::AbsolutePath(payload.key().clone())
                    };

                    let get_parquet_files_start = Instant::now();
                    let parquet_files = s3_operator
                        .get_list_of_parquet_files_from_s3(&load_parquet_files_payload)
                        .await;
                    let get_parquet_files_duration =
                        beautify_duration(get_parquet_files_start.elapsed());
                    info!(
                        "Get parquet files from S3 duration: {}",
                        get_parquet_files_duration
                    );

                    // For each `.parquet` file in S3, we create a Dataframe
                    // that will be inserted in the target database, after getting
                    // anonymized.
                    info!("{}", "Reading Parquet files from S3".bold().green());
                    for file in &parquet_files.unwrap() {
                        let create_dataframe_payload = CreateDataframePayload {
                            bucket_name: payload.bucket_name().clone(),
                            key: file.file_name.to_string(),
                            database_name: payload.database_name().clone(),
                            schema_name: payload.schema_name().clone(),
                            table_name: table.clone(),
                        };

                        let create_df_start = Instant::now();
                        let current_df = dataframe_operator
                            .create_dataframe_from_parquet_file(&create_dataframe_payload)
                            .await
                            .map_err(|e| {
                                panic!("Error reading Parquet file: {:?}", e);
                            })
                            .unwrap();
                        let create_df_duration = beautify_duration(create_df_start.elapsed());
                        info!("Creating DF for table {table} took: {create_df_duration}");

                        let current_df = if let Some(current_df) = current_df {
                            current_df
                        } else {
                            continue;
                        };

                        // This branch will operate on LOAD `.parquet` files,
                        // whereas the next one is responsible for the CDC files.
                        if file.is_load_file() {
                            info!("Processing LOAD file: {:?}", file);
                            // Check if the schema of the table is the same as the schema of the Parquet file
                            // in case of altered column names or dropped columns
                            let df_column_fields = current_df.get_columns();

                            let has_schema_diff = df_column_fields
                                .iter()
                                .filter(|field| {
                                    field.name() != "Op"
                                        && field.name() != "_dms_ingestion_timestamp"
                                })
                                .any(|field| !source_table_columns.contains_key(field.name().as_str()));

                            // Early exit if we detect a schema change. In order to mitigate that,
                            // we will trigger a new full load through DMS.
                            if has_schema_diff {
                                panic!(
                                    "Schema of table is not the same as the schema of the Parquet file"
                                );
                            }

                            // Prepare for the insertion of the Dataframe in the target
                            // database.
                            let insert_dataframe_payload = InsertDataframePayload {
                                database_name: payload.database_name().clone(),
                                schema_name: payload.schema_name().clone(),
                                table_name: table.clone(),
                            };
                            let insert_dataframe_start = Instant::now();
                            let insert_result = superuser_postgres_operator
                                .insert_dataframe_in_target_db(
                                    &current_df,
                                    &insert_dataframe_payload,
                                )
                                .await;

                            match insert_result {
                                Ok(_) => {
                                    info!("Successfully inserted LOAD file into table");
                                }
                                Err(e) => {
                                    panic!(
                                        "Failed to insert LOAD file into table -> {}: {:?}",
                                        table.clone(),
                                        e
                                    );
                                }
                            }
                            let insert_dataframe_duration =
                                beautify_duration(insert_dataframe_start.elapsed());
                            info!(
                                "Insert DF {} duration: {}",
                                table, insert_dataframe_duration
                            );
                        } else {
                            info!("Processing CDC file: {:?}", file);
                            let primary_keys = primary_key_list.join(",");

                            let upsert_dataframe_payload = UpsertDataframePayload {
                                database_name: payload.database_name().clone(),
                                schema_name: payload.schema_name().clone(),
                                table_name: table.clone(),
                                primary_key: primary_keys.clone(),
                            };

                            superuser_postgres_operator
                                .upsert_dataframe_in_target_db(
                                    &current_df,
                                    &upsert_dataframe_payload,
                                )
                                .await
                                .unwrap_or_else(|_| {
                                    panic!("Failed to upsert CDC file {:?} into table", file)
                                });
                        }

                        drop(current_df);
                    }

                    drop(s3_operator);
                    drop(dataframe_operator);

                    let elapsed = beautify_duration(start.elapsed());

                    info!(
                        "{}",
                        format!("Snapshot completed for table {table} in: {elapsed}")
                            .yellow()
                            .bold(),
                    );
                }
            })
            .collect::<Vec<_>>();

        use futures::FutureExt;
        use futures::StreamExt;
        use futures::stream::{self};

        let stream = stream::iter(anonymized_tables)
            .map(|future| future.boxed())
            .buffer_unordered(num_of_buffers());

        // Collect results, ensuring at most [num_of_buffers()] futures run concurrently
        stream.for_each(|_| async {}).await;

        info!("{}", "Snapshotting completed...".bold().blue());
    }

    /// Finalizes the snapshot by updating the sequences in the target database.
    ///
    /// # Arguments
    ///
    /// * `target_pool` - The database pool for the target database.
    /// * `datab ase_name` - The name of the target database.
    /// * `schema_name` - The name of the schema in the target database.
    /// * `application_users` - Any application users to be granted with permissions,
    ///   in the target database.
    pub async fn finalize_snapshot(
        target_pool: Pool,
        database_name: &str,
        schema_name: &str,
        application_users: Vec<String>,
    ) {
        let target_db_finalizer = TargetDBFinalizer::builder()
            .target_db_pool(target_pool)
            .build();

        // Update sequence values
        target_db_finalizer
            .update_sequence_values(schema_name)
            .await;

        // Grant permissions to application users
        target_db_finalizer
            .grant_permissions_to_application_users(database_name, schema_name, application_users)
            .await;
    }
}

// Controls the number of max parallel processed
// Dataframes for insertion.
fn num_of_buffers() -> usize {
    env::var("NUM_OF_BUFFERS")
        .unwrap_or_else(|_| "80".to_string())
        .parse::<usize>()
        .unwrap()
}
