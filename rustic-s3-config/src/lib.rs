use std::env;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use aws_config::{Region, SdkConfig, default_provider::credentials::DefaultCredentialsChain};
use aws_sdk_s3::Client as S3Client;
use aws_sdk_s3::config::{ProvideCredentials, StalledStreamProtectionConfig};
use polars::io::cloud::credential_provider::{
    AwsCredential, ObjectStoreCredential, PlCredentialProvider,
};
use polars::io::cloud::{AmazonS3ConfigKey, CloudOptions};
use polars::prelude::{PolarsResult, polars_err};
use tokio::runtime::Handle;
use tokio::sync::OnceCell;

const S3_AWS_REGION: &str = "eu-west-1";

static SDK_CONFIG: OnceCell<SdkConfig> = OnceCell::const_new();
static CLOUD_OPTIONS: OnceCell<CloudOptions> = OnceCell::const_new();

/// Process-wide aws `SdkConfig`: `S3_BUCKET_REGION` region, optional
/// `S3_VPC_ENDPOINT` endpoint, default credential chain. Loaded once and
/// shared by the S3 client and the Polars scans.
pub async fn sdk_config() -> &'static SdkConfig {
    SDK_CONFIG
        .get_or_init(|| async {
            let mut loader = aws_config::from_env()
                .region(Region::new(s3_bucket_region()))
                .credentials_provider(DefaultCredentialsChain::builder().build().await)
                // Intentionally disable the stalled stream protection
                .stalled_stream_protection(StalledStreamProtectionConfig::disabled());
            if let Ok(value) = env::var("S3_VPC_ENDPOINT") {
                loader = loader.endpoint_url(value);
            }
            loader.load().await
        })
        .await
}

pub async fn create_s3_client() -> S3Client {
    S3Client::new(sdk_config().await)
}

type CredentialFuture =
    Pin<Box<dyn Future<Output = PolarsResult<(ObjectStoreCredential, u64)>> + Send + Sync>>;

/// Polars [`CloudOptions`] wired to [`sdk_config`]: the same credential chain
/// (env vars / IRSA web-identity / IMDS / SSO), region and endpoint as the S3
/// client. Memoized on purpose: the credential provider's address is part of
/// Polars' object-store cache key, so a single instance keeps one object store
/// and HTTP client for the whole run, and the provider is only re-invoked when
/// the credentials expire.
pub async fn polars_cloud_options() -> CloudOptions {
    CLOUD_OPTIONS
        .get_or_init(|| async {
            let sdk_config = sdk_config().await;
            let provider = sdk_config
                .credentials_provider()
                .expect("sdk_config() always sets a credentials provider");
            // Polars drives the credential future on its own runtime; the SDK
            // call is spawned back onto the app runtime it was built for.
            let handle = Handle::current();

            let credential_provider = PlCredentialProvider::from_func(move || {
                let p = provider.clone();
                let jh = handle.spawn(async move { p.provide_credentials().await });
                let fut: CredentialFuture = Box::pin(async move {
                    let creds = jh
                        .await
                        .map_err(|e| polars_err!(ComputeError: "credential task failed: {e}"))?
                        .map_err(
                            |e| polars_err!(ComputeError: "failed to resolve AWS credentials: {e}"),
                        )?;

                    // Seconds since UNIX_EPOCH, 60 s early so Polars refreshes
                    // before the SDK's own expiry, but never in the past (Polars
                    // rejects that); `u64::MAX` means "never expires".
                    let now = SystemTime::now()
                        .duration_since(UNIX_EPOCH)
                        .map(|d| d.as_secs())
                        .unwrap_or(0);
                    let expiry = creds
                        .expiry()
                        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                        .map(|d| d.as_secs().saturating_sub(60).max(now))
                        .unwrap_or(u64::MAX);

                    let credential = AwsCredential {
                        key_id: creds.access_key_id().to_string(),
                        secret_key: creds.secret_access_key().to_string(),
                        token: creds.session_token().map(str::to_string),
                    };
                    Ok((ObjectStoreCredential::Aws(Arc::new(credential)), expiry))
                });
                fut
            });

            // ponytail: the endpoint is passed through as-is. The SDK sends
            // virtual-hosted requests to a custom endpoint while object_store
            // defaults to path-style; AWS interface endpoints (`bucket.vpce-...`)
            // accept both, so no addressing-style mapping until one needs it.
            let aws = sdk_config
                .region()
                .map(|region| (AmazonS3ConfigKey::Region, region.to_string()))
                .into_iter()
                .chain(
                    sdk_config
                        .endpoint_url()
                        .map(|endpoint| (AmazonS3ConfigKey::Endpoint, endpoint.to_string())),
                );

            CloudOptions::default()
                .with_credential_provider(Some(credential_provider))
                .with_aws(aws)
        })
        .await
        .clone()
}

fn s3_bucket_region() -> String {
    env::var("S3_BUCKET_REGION").unwrap_or(String::from(S3_AWS_REGION))
}
