use std::path::Path;

use aws_config::BehaviorVersion;
use aws_sdk_s3::{Client, primitives::ByteStream, primitives::SdkBody};
use rustic_shell::shell_command_executor::ShellCommandExecutor;
use tracing::{error, info};

pub struct MongoDataExporter {
    pub mongo_uri: String,
    pub s3_path: String,
    pub database_name: String,
    pub exclude_collections: Vec<String>,
    pub include_collections: Vec<String>,
}

impl MongoDataExporter {
    const ZSTD_ARCHIVE_EXTENSION: &str = "tar.zst";
    const ZSTD_ARCHIVE_OPTIONS: &str = "-acf";

    pub fn new(
        mongo_uri: String,
        s3_path: String,
        database_name: String,
        exclude_collections: Vec<String>,
        include_collections: Vec<String>,
    ) -> Self {
        Self {
            mongo_uri,
            s3_path,
            database_name,
            exclude_collections,
            include_collections,
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

        // Get file size for logging
        let file_metadata = std::fs::metadata(file_path).expect("Failed to read file metadata");
        let file_size = file_metadata.len();
        let file_size_mb = file_size as f64 / (1024.0 * 1024.0);
        let file_size_gb = file_size as f64 / (1024.0 * 1024.0 * 1024.0);

        info!(
            "File size: {} bytes ({:.2} MB, {:.2} GB)",
            file_size, file_size_mb, file_size_gb
        );

        let s3_bucket_key = format!(
            "{s3_bucket_key}/mongo-{}.{}",
            self.database_name,
            Self::ZSTD_ARCHIVE_EXTENSION
        );

        info!("Will upload file to S3 bucket {s3_bucket_name} with key {s3_bucket_key}");

        // Use multipart upload for files larger than 5GB
        const MAX_SINGLE_UPLOAD_SIZE: u64 = 5 * 1024 * 1024 * 1024; // 5GB in bytes

        if file_size > MAX_SINGLE_UPLOAD_SIZE {
            info!("File size exceeds 5GB, using multipart upload");
            self.upload_multipart_to_s3(
                &client,
                s3_bucket_name,
                &s3_bucket_key,
                file_path,
                file_size,
            )
            .await;
        } else {
            info!("File size is within 5GB limit, using single upload");
            let file_stream = ByteStream::from_path(file_path)
                .await
                .expect("Failed to read file");

            client
                .put_object()
                .bucket(s3_bucket_name)
                .key(s3_bucket_key)
                .body(file_stream)
                .send()
                .await
                .expect("Failed to upload file to S3");
        }

        info!("Successfully uploaded file to S3");
    }

    async fn upload_multipart_to_s3(
        &self,
        client: &Client,
        bucket_name: &str,
        key: &str,
        file_path: &str,
        file_size: u64,
    ) {
        // Initialize multipart upload
        let create_multipart_upload_output = client
            .create_multipart_upload()
            .bucket(bucket_name)
            .key(key)
            .send()
            .await
            .expect("Failed to create multipart upload");

        let upload_id = create_multipart_upload_output
            .upload_id()
            .expect("Upload ID not found")
            .to_string();

        info!("Created multipart upload with ID: {}", upload_id);

        // Calculate part size (minimum 1GB, maximum 5GB per part)
        const MIN_PART_SIZE: u64 = 1024 * 1024 * 1024; // 1GB
        const MAX_PART_SIZE: u64 = 5 * 1024 * 1024 * 1024; // 5GB
        let part_size = std::cmp::max(MIN_PART_SIZE, file_size / 10); // Aim for max 10 parts
        let part_size = std::cmp::min(part_size, MAX_PART_SIZE);

        let mut part_number = 1;
        let mut uploaded_parts = Vec::new();
        let mut file = tokio::fs::File::open(file_path)
            .await
            .expect("Failed to open file");

        info!("Using part size: {} bytes", part_size);

        // Read file in chunks and upload each part
        loop {
            let mut buffer = vec![0u8; part_size as usize];
            let bytes_read = tokio::io::AsyncReadExt::read(&mut file, &mut buffer)
                .await
                .expect("Failed to read file");

            if bytes_read == 0 {
                break;
            }

            buffer.truncate(bytes_read);
            let body = ByteStream::from(SdkBody::from(buffer));

            info!("Uploading part {} ({} bytes)", part_number, bytes_read);

            let upload_part_output = client
                .upload_part()
                .bucket(bucket_name)
                .key(key)
                .part_number(part_number)
                .upload_id(&upload_id)
                .body(body)
                .send()
                .await
                .expect("Failed to upload part");

            let etag = upload_part_output
                .e_tag()
                .expect("ETag not found")
                .to_string();

            uploaded_parts.push(
                aws_sdk_s3::types::CompletedPart::builder()
                    .part_number(part_number)
                    .e_tag(etag)
                    .build(),
            );

            part_number += 1;
        }

        // Complete multipart upload
        info!(
            "Completing multipart upload with {} parts",
            uploaded_parts.len()
        );

        client
            .complete_multipart_upload()
            .bucket(bucket_name)
            .key(key)
            .upload_id(&upload_id)
            .multipart_upload(
                aws_sdk_s3::types::CompletedMultipartUpload::builder()
                    .set_parts(Some(uploaded_parts))
                    .build(),
            )
            .send()
            .await
            .expect("Failed to complete multipart upload");

        info!("Multipart upload completed successfully");
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
            format!("--db={}", self.database_name),
        ];

        command_parts.extend(
            self.include_collections
                .iter()
                .map(|collection| format!("--collection={collection}")),
        );


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
