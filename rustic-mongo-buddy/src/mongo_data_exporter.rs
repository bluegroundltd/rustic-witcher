use std::path::Path;

use aws_config::BehaviorVersion;
use aws_sdk_s3::{Client, primitives::ByteStream};
use rustic_shell::shell_command_executor::ShellCommandExecutor;
use tracing::{error, info};

pub struct MongoDataExporter {
    pub mongo_uri: String,
    pub s3_path: String,
    pub database_name: String,
    pub exclude_collections: Vec<String>,
}

impl MongoDataExporter {
    const ZSTD_ARCHIVE_EXTENSION: &str = "tar.zst";
    const ZSTD_ARCHIVE_OPTIONS: &str = "-acf";

    pub fn new(
        mongo_uri: String,
        s3_path: String,
        database_name: String,
        exclude_collections: Vec<String>,
    ) -> Self {
        Self {
            mongo_uri,
            s3_path,
            database_name,
            exclude_collections,
        }
    }

    pub async fn export_data(&self) {
        let db_name_from_uri = self.mongo_uri.split('/').next_back().unwrap();
        if db_name_from_uri != self.database_name {
            error!(
                "Database name in URI {} does not match provided database name {}",
                db_name_from_uri, self.database_name
            );
            return;
        }

        let local_output_folder = "/tmp/mongo-dump";
        let archived_dump = format!("/tmp/mongo-dump.{}", Self::ZSTD_ARCHIVE_EXTENSION);
        let archived_dump = archived_dump.as_str();

        let mongo_host = self.mongo_uri.split('@').collect::<Vec<_>>()[1];

        info!("Dumping mongo data from {mongo_host} to {local_output_folder}");

        self.execute_mongo_dump(local_output_folder).await;
        Self::archive(archived_dump, local_output_folder).await;

        let s3_path = Path::new(&self.s3_path);
        let s3_bucket_name = s3_path
            .components()
            .nth(1)
            .unwrap()
            .as_os_str()
            .to_str()
            .unwrap();
        let s3_bucket_key = s3_path
            .components()
            .skip(2)
            .map(|comp| comp.as_os_str().to_str().unwrap())
            .collect::<Vec<&str>>()
            .join("/");

        self.upload_to_s3(s3_bucket_name, s3_bucket_key.as_str(), archived_dump)
            .await;

        info!("Cleaning up local files");

        std::fs::remove_file(archived_dump).expect("Failed to remove archived dump file");
        std::fs::remove_dir_all(local_output_folder)
            .expect("Failed to cleanup local output folder");
    }

    async fn upload_to_s3(&self, s3_bucket_name: &str, s3_bucket_key: &str, file_path: &str) {
        let config = aws_config::defaults(BehaviorVersion::latest()).load().await;
        let client = Client::new(&config);

        info!("Created S3 client!");

        let file_stream = ByteStream::from_path(file_path)
            .await
            .expect("Failed to read file");

        let s3_bucket_key = format!(
            "{s3_bucket_key}/mongo-{}.{}",
            self.database_name,
            Self::ZSTD_ARCHIVE_EXTENSION
        );

        info!("Will upload file to S3 bucket {s3_bucket_name} with key {s3_bucket_key}");

        client
            .put_object()
            .bucket(s3_bucket_name)
            .key(s3_bucket_key)
            .body(file_stream)
            .send()
            .await
            .expect("Failed to upload file to S3");
    }

    async fn archive(archived_dump: &str, local_output_folder: &str) {
        let archive_command = format!(
            "tar {} {archived_dump} -C {local_output_folder} .",
            Self::ZSTD_ARCHIVE_OPTIONS,
        );
        ShellCommandExecutor::execute_cmd(archive_command, None).await;
        info!("Archived dump: {archived_dump}");
    }

    async fn execute_mongo_dump(&self, local_output_folder: &str) {
        let mut command_parts = vec![
            String::from("mongodump"),
            format!("--uri={}", self.mongo_uri),
            format!("--out={local_output_folder}"),
            String::from("--compressors=snappy"),
            String::from("--gzip"),
            String::from("--readPreference=secondary"),
        ];

        command_parts.extend(
            self.exclude_collections
                .iter()
                .map(|collection| format!("--excludeCollection={collection}")),
        );

        let mongo_dump_command = command_parts.join(" ");
        ShellCommandExecutor::execute_cmd(&mongo_dump_command, None).await;
        info!("Mongo dump finished!");
    }
}
