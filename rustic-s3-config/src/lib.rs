use aws_config::{Region, default_provider::credentials::DefaultCredentialsChain};
use aws_sdk_s3::{Client as S3Client, config::StalledStreamProtectionConfig};
use std::env;

const S3_AWS_REGION: &str = "eu-west-1";

pub async fn create_s3_client() -> S3Client {
    let config = match env::var("S3_VPC_ENDPOINT") {
        Ok(value) => {
            aws_config::from_env()
                .region(Region::new(s3_bucket_region()))
                .credentials_provider(DefaultCredentialsChain::builder().build().await)
                .endpoint_url(value)
                // Intentionally disable the stalled stream protection
                .stalled_stream_protection(StalledStreamProtectionConfig::disabled())
                .load()
                .await
        }
        Err(_) => {
            aws_config::from_env()
                .region(Region::new(s3_bucket_region()))
                .credentials_provider(DefaultCredentialsChain::builder().build().await)
                // Intentionally disable the stalled stream protection
                .stalled_stream_protection(StalledStreamProtectionConfig::disabled())
                .load()
                .await
        }
    };
    S3Client::new(&config)
}

fn s3_bucket_region() -> String {
    env::var("S3_BUCKET_REGION").unwrap_or(String::from(S3_AWS_REGION))
}
