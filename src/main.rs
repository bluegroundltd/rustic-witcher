use std::env;

use anyhow::Result;
use colored::Colorize;

use clap::{Parser, Subcommand};
use dms_cdc_operator::cdc::cdc_operator_mode::ModeValueEnum;
use dms_cdc_operator::postgres::postgres_operator::PostgresOperator;
use dms_cdc_operator::{
    cdc::{cdc_operator_payload::CDCOperatorPayload, snapshot_payload::CDCOperatorSnapshotPayload},
    postgres::postgres_operator_impl::PostgresOperatorImpl,
};

use rustic_result_validator::config_loader::loader::ValidationConfigLoader;
use rustic_result_validator::validator::result_validator::ResultValidator;
use rustic_target_db::prepare_db_config;
use tracing::info;

use deadpool_postgres::Runtime;
use deadpool_postgres::tokio_postgres::NoTls;

use crate::execution_payload::ExecutionPayload;

mod execution_payload;

fn included_tables_path_parser(path: &str) -> Result<String> {
    Ok(format!("configuration_data/inclusions/{path}"))
}

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
#[command(propagate_version = true)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    Anonymize {
        /// S3 Bucket name where the CDC files are stored
        #[arg(long, required = true)]
        bucket_name: String,
        /// S3 Prefix where the files are stored
        /// Example: data/landing/rds/mydb
        #[arg(long, required = true)]
        s3_prefix: String,
        /// Url of the database to validate the CDC files
        /// Example: postgres://postgres:postgres@localhost:5432/mydb
        #[arg(long, required = true)]
        source_database_name: String,
        /// Application users for permission assignment on target DB
        #[arg(long, value_delimiter = ',', num_args = 0.., required = false)]
        target_application_users: Vec<String>,
        /// Schema of database to validate against S3 files
        #[arg(long, required = false, default_value = "public")]
        database_schema: String,
        /// List of tables to include for validatation against S3 files
        #[arg(long, value_delimiter = ',', num_args = 0.., required = false, conflicts_with("excluded_tables"), group = "included_tables_group")]
        included_tables: Vec<String>,
        /// List of tables to include for validatation against S3 files (file form)
        #[arg(
            long,
            required = false,
            value_parser = included_tables_path_parser,
            conflicts_with("included_tables"),
            group = "included_tables_group"
        )]
        included_tables_from_file: String,
        /// List of tables to exclude for validatation against S3 files
        #[arg(long, value_delimiter = ',', num_args = 0.., required = false, conflicts_with("included_tables_group"))]
        excluded_tables: Vec<String>,
        /// Mode to load Parquet files
        /// Example: DateAware
        /// Example: AbsolutePath
        /// Example: FullLoadOnly
        #[arg(long, required = false, default_value = "full-load-only")]
        #[clap(value_enum)]
        mode: ModeValueEnum,
        /// Maximum connection pool size
        #[arg(long, required = false, default_value = "100")]
        max_connections: u32,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();

    let info = auto_allocator::get_allocator_info();
    info!("✅ Using: {:?}", info.allocator_type);
    info!("💡 {}", info.reason);

    let cli = Cli::parse();
    let (execution_payload, cdc_operator_payload) = match cli.command {
        Commands::Anonymize {
            bucket_name,
            s3_prefix,
            source_database_name,
            target_application_users,
            database_schema,
            included_tables,
            included_tables_from_file,
            excluded_tables,
            mode,
            max_connections,
            ..
        } => {
            info!("Will include tables from file: {included_tables_from_file}");

            let included_tables = if included_tables.is_empty() {
                std::fs::read_to_string(included_tables_from_file)
                    .expect("Failed to read file")
                    .lines()
                    .map(String::from)
                    .collect()
            } else {
                included_tables
            };

            let record_reduction_enabled: bool = env::var("RECORD_REDUCTION_ENABLED")
                .unwrap_or_else(|_| "false".to_string())
                .parse()
                .unwrap();

            info!("Record reduction is: {record_reduction_enabled}");

            let execution_payload = ExecutionPayload::builder()
                .target_application_users(target_application_users)
                .build();

            // Build SOURCE_POSTGRES_URL
            let source_postgres_url = format!(
                "{}_{}_SOURCE_POSTGRES_URL",
                source_database_name.to_uppercase(),
                database_schema.to_uppercase()
            );
            info!("Will source from: {source_postgres_url}!");
            let source_postgres_url =
                env::var(source_postgres_url).expect("Source Postgres URL could not be loaded");
            let source_postgres_url = format!("{source_postgres_url}/{source_database_name}");

            // Build TARGET_POSTGRES_URL
            let target_postgres_url = format!(
                "{}_{}_TARGET_POSTGRES_URL",
                source_database_name.to_uppercase(),
                database_schema.to_uppercase()
            );

            info!("Will target to: {target_postgres_url}!");
            let target_postgres_url =
                env::var(target_postgres_url).expect("Target Postgres URL could not be loaded");
            // Intentionally selecting `source_database_name` here as the target database name,
            // since they will be the same.
            let target_postgres_url = format!("{target_postgres_url}/{source_database_name}");

            let cdc_operator_payload = CDCOperatorPayload::builder()
                .bucket_name(bucket_name)
                .s3_prefix(s3_prefix)
                .source_postgres_url(source_postgres_url)
                .target_postgres_url(target_postgres_url)
                .database_schema(database_schema)
                .included_tables(included_tables)
                .excluded_tables(excluded_tables)
                .mode(mode)
                .start_date(None)
                .stop_date(None)
                .chunk_size(1000)
                .max_connections(max_connections)
                .start_position(0)
                .only_datadiff(false)
                .only_snapshot(false)
                .accept_invalid_certs_first_db(false)
                .accept_invalid_certs_second_db(false)
                .build();

            (execution_payload, cdc_operator_payload)
        }
    };

    // Connect to the Postgres database
    info!("{}", "Connecting to source Postgres DB".bold().green());

    let cdc_operator_snapshot_payload = CDCOperatorSnapshotPayload::builder()
        .bucket_name(cdc_operator_payload.bucket_name())
        .key(cdc_operator_payload.s3_prefix())
        .database_name(cdc_operator_payload.database_name())
        .schema_name(cdc_operator_payload.schema_name())
        .included_tables(cdc_operator_payload.included_tables().to_vec())
        .excluded_tables(cdc_operator_payload.excluded_tables().to_vec())
        .mode(cdc_operator_payload.mode())
        .maybe_stop_date(cdc_operator_payload.stop_date().map(|x| x.to_string()))
        .maybe_start_date(cdc_operator_payload.stop_date().map(|x| x.to_string()))
        .source_postgres_url(cdc_operator_payload.source_postgres_url().to_string())
        .target_postgres_url(cdc_operator_payload.target_postgres_url().to_string())
        .build();

    // Prepare target DB for snapshot
    _ = rustic_cdc_operator::cdc_operator::CDCOperator::prepare_for_snapshot(
        &cdc_operator_snapshot_payload,
    )
    .await;

    // Create source postgres operator
    let source_cfg = prepare_db_config(cdc_operator_payload.source_postgres_url().to_string());
    let source_pool = source_cfg
        .create_pool(Some(Runtime::Tokio1), NoTls)
        .unwrap();
    let source_postgres_operator = PostgresOperatorImpl::builder().pool(source_pool).build();

    // After this point we need to use the DB role that has
    // session_replication_role set to replica
    let superuser_url = env::var("SUPERUSER_URL").unwrap();
    let superuser_url = format!("{superuser_url}/{}", cdc_operator_payload.database_name());

    let target_cfg = prepare_db_config(superuser_url);
    let target_pool = target_cfg
        .create_pool(Some(Runtime::Tokio1), NoTls)
        .unwrap();
    let target_postgres_operator = PostgresOperatorImpl::builder()
        .pool(target_pool.clone())
        .build();

    // Create an S3 client
    info!("{}", "Creating S3 client".bold().green());
    let s3_client = rustic_s3_config::create_s3_client().await;

    // Snapshot the source database & anonymize the data
    _ = rustic_cdc_operator::cdc_operator::CDCOperator::snapshot(
        &cdc_operator_snapshot_payload,
        &source_postgres_operator,
        &target_postgres_operator,
        &s3_client,
    )
    .await;

    _ = rustic_cdc_operator::cdc_operator::CDCOperator::finalize_snapshot(
        target_pool.clone(),
        cdc_operator_payload.database_name().as_str(),
        cdc_operator_payload.schema_name(),
        execution_payload.target_application_users(),
    )
    .await;

    // Read the validations configs
    let validation_config_loader = ValidationConfigLoader::builder()
        .database_name(cdc_operator_payload.database_name())
        .schema_name(cdc_operator_payload.schema_name().to_string())
        .build();
    let validation_configs = validation_config_loader.load_validations_config();

    // Execute the validations
    let result_validator = ResultValidator::builder()
        .database_name(cdc_operator_payload.database_name())
        .schema_name(cdc_operator_payload.schema_name().to_string())
        .target_db_pool(target_pool)
        .validations(validation_configs)
        .build();
    _ = result_validator.validate().await;

    // Close the connection pool
    info!("{}", "Closing connection pool".bold().green());
    source_postgres_operator.close_connection_pool().await;
    target_postgres_operator.close_connection_pool().await;

    Ok(())
}
