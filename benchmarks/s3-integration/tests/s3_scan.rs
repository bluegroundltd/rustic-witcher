//! End-to-end test of the operator's S3 `scan_parquet` path against a local
//! Floci S3 endpoint. Verifies that predicate + slice pushdown read back the
//! correct rows, and that polars authenticates to a custom endpoint using the
//! aws-sdk credential chain — through the operator's real
//! `rustic_s3_config::polars_cloud_options()`.
//!
//! Run with Floci up (`docker compose up -d`):
//!   AWS_ENDPOINT_URL=http://localhost:4566 cargo test -- --nocapture

use aws_sdk_s3::primitives::ByteStream;
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
        std::env::set_var("S3_BUCKET_REGION", "us-east-1"); // rustic_s3_config: defaults to eu-west-1
        std::env::set_var("AWS_ENDPOINT_URL", ENDPOINT); // picked up by rustic_s3_config::sdk_config()
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
    // Region + endpoint come from the env set above; plain-http to the custom
    // endpoint is enabled via AWS_ALLOW_HTTP (object_store ClientOption).
    let opts = rustic_s3_config::polars_cloud_options().await;
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
    assert_eq!(
        filtered.height(),
        ROWS / 5000,
        "predicate pushdown row count"
    );
    assert_eq!(filtered.width(), 3, "all columns present");

    // --- act 2: slice (n_rows) pushdown — record reduction ---
    let opts = rustic_s3_config::polars_cloud_options().await;
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
