//! End-to-end test of the operator's S3 `scan_parquet` path against a local
//! Floci S3 endpoint. Verifies that predicate + slice pushdown read back the
//! correct rows, and that polars authenticates to a custom endpoint using the
//! aws-sdk credential chain — the same wiring `build_cloud_options` uses.
//!
//! Run with Floci up (`docker compose up -d`):
//!   AWS_ENDPOINT_URL=http://localhost:4566 cargo test -- --nocapture

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use aws_sdk_s3::config::ProvideCredentials;
use aws_sdk_s3::primitives::ByteStream;
use polars::io::cloud::credential_provider::{
    AwsCredential, ObjectStoreCredential, PlCredentialProvider,
};
use polars::io::cloud::{AmazonS3ConfigKey, CloudOptions};
use polars::polars_utils::pl_path::PlRefPath;
use polars::prelude::*;

const ENDPOINT: &str = "http://localhost:4566";
const BUCKET: &str = "rustic-witcher-it";
const KEY: &str = "data/landing/LOAD00000001.parquet";
const ROWS: usize = 100_000;

fn set_env() {
    // SAFETY: single-threaded test setup before any S3 client is built.
    unsafe {
        std::env::set_var("AWS_ACCESS_KEY_ID", "test");
        std::env::set_var("AWS_SECRET_ACCESS_KEY", "test");
        std::env::set_var("AWS_REGION", "us-east-1");
        std::env::set_var("AWS_ENDPOINT_URL", ENDPOINT);
        std::env::set_var("AWS_ALLOW_HTTP", "true"); // object_store: permit plain-http endpoint
    }
}

fn make_parquet_bytes() -> Vec<u8> {
    let id: Vec<i64> = (0..ROWS as i64).collect();
    let name: Vec<String> = (0..ROWS).map(|i| format!("name_{}", i % 5000)).collect();
    let payload: Vec<String> = (0..ROWS).map(|i| format!("v_{}", i % 1000)).collect();
    let mut df = df!["id" => id, "name" => name, "payload" => payload].unwrap();

    let mut buf = Vec::new();
    ParquetWriter::new(&mut buf)
        .with_row_group_size(Some(16 * 1024))
        .finish(&mut df)
        .unwrap();
    buf
}

async fn s3_client() -> aws_sdk_s3::Client {
    let cfg = aws_config::load_defaults(aws_config::BehaviorVersion::latest()).await;
    let s3_cfg = aws_sdk_s3::config::Builder::from(&cfg)
        .endpoint_url(ENDPOINT)
        .force_path_style(true) // localhost needs path-style addressing
        .build();
    aws_sdk_s3::Client::from_conf(s3_cfg)
}

/// Mirror of the operator's `build_cloud_options`, plus the endpoint so the scan
/// targets Floci instead of real AWS.
async fn build_cloud_options() -> CloudOptions {
    let sdk_config = aws_config::load_defaults(aws_config::BehaviorVersion::latest()).await;
    let creds = sdk_config
        .credentials_provider()
        .unwrap()
        .provide_credentials()
        .await
        .unwrap();

    let aws_credential = Arc::new(AwsCredential {
        key_id: creds.access_key_id().to_string(),
        secret_key: creds.secret_access_key().to_string(),
        token: creds.session_token().map(str::to_string),
    });

    let credential_provider = PlCredentialProvider::from_func(move || {
        let aws_credential = aws_credential.clone();
        let fut: Pin<
            Box<dyn Future<Output = PolarsResult<(ObjectStoreCredential, u64)>> + Send + Sync>,
        > = Box::pin(std::future::ready(Ok((
            ObjectStoreCredential::Aws(aws_credential),
            u64::MAX,
        ))));
        fut
    });

    CloudOptions::default()
        .with_credential_provider(Some(credential_provider))
        // Plain-http to a custom endpoint is enabled via the AWS_ALLOW_HTTP env var
        // (object_store ClientOption, not an AmazonS3ConfigKey).
        .with_aws([
            (AmazonS3ConfigKey::Region, "us-east-1".to_string()),
            (AmazonS3ConfigKey::Endpoint, ENDPOINT.to_string()),
            (AmazonS3ConfigKey::VirtualHostedStyleRequest, "false".to_string()),
        ])
}

// Multi-threaded runtime mirrors the app's `#[tokio::main]`. polars cloud
// `collect()` calls `block_in_place`, which panics on a current-thread runtime
// ("can call blocking only when running on the multi-threaded runtime").
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn scan_parquet_from_floci_s3_with_pushdown() {
    set_env();

    // --- arrange: create bucket + upload a parquet object ---
    let client = s3_client().await;
    let _ = client.create_bucket().bucket(BUCKET).send().await; // ignore "already exists"
    client
        .put_object()
        .bucket(BUCKET)
        .key(KEY)
        .body(ByteStream::from(make_parquet_bytes()))
        .send()
        .await
        .expect("put_object failed — is Floci up on :4566?");

    let path = format!("s3://{BUCKET}/{KEY}");

    // --- act 1: predicate pushdown (selective filter over the network) ---
    let opts = build_cloud_options().await;
    let args = ScanArgsParquet {
        cloud_options: Some(opts),
        glob: false,
        ..Default::default()
    };
    let lazy = LazyFrame::scan_parquet(PlRefPath::new(path.as_str()), args)
        .expect("scan_parquet failed")
        .filter(col("name").eq(lit("name_42")));
    // Mirror the operator: collect() off the async reactor on a blocking thread.
    let filtered = tokio::task::spawn_blocking(move || lazy.collect())
        .await
        .unwrap()
        .expect("collect failed");

    // name_{i%5000} == name_42 → every 5000th row → ROWS/5000 matches.
    assert_eq!(filtered.height(), ROWS / 5000, "predicate pushdown row count");
    assert_eq!(filtered.width(), 3, "all columns present");

    // --- act 2: slice (n_rows) pushdown — record reduction ---
    let opts = build_cloud_options().await;
    let args = ScanArgsParquet {
        n_rows: Some(1000),
        cloud_options: Some(opts),
        glob: false,
        ..Default::default()
    };
    let lazy = LazyFrame::scan_parquet(PlRefPath::new(path.as_str()), args).unwrap();
    let sliced = tokio::task::spawn_blocking(move || lazy.collect())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(sliced.height(), 1000, "n_rows pushdown");

    println!(
        "OK: filtered={} rows, sliced={} rows from s3://{BUCKET}/{KEY}",
        filtered.height(),
        sliced.height()
    );
}
