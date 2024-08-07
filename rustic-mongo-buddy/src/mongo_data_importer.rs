use std::{fs::File, io::Write as _, path::Path};

use aws_config::BehaviorVersion;
use aws_sdk_s3::Client;
use rustic_shell::shell_command_executor::ShellCommandExecutor;
use tracing::{error, info};

pub struct MongoDataImporter {
    pub mongo_uri: String,
    pub s3_path: String,
    pub database_name: String,
}

impl MongoDataImporter {
    pub fn new(mongo_uri: String, s3_path: String, database_name: String) -> Self {
        Self {
            mongo_uri,
            s3_path,
            database_name,
        }
    }

    pub async fn import_data(&self) {
        if self.mongo_uri.contains("prod") || self.mongo_uri.contains("production") {
            error!("Cannot import data to production environment");
            return;
        }

        let extracted_mongo_files_location = format!("/tmp/mongo-dump/{}", self.database_name);
        let extracted_mongo_files_location = extracted_mongo_files_location.as_str();

        std::fs::create_dir_all(extracted_mongo_files_location)
            .expect("Failed to create extraction directory");

        let compressed_mongo_dataset = self.download_dump_file().await;

        info!("Extracting {compressed_mongo_dataset} to {extracted_mongo_files_location}");
        Self::untar_file(&compressed_mongo_dataset, extracted_mongo_files_location).await;

        let mongo_host = self.mongo_uri.split('@').collect::<Vec<_>>()[1];

        info!("Importing {extracted_mongo_files_location} to {mongo_host}");
        self.execute_mongo_restore(extracted_mongo_files_location)
            .await;

        info!("Deleting tar file {compressed_mongo_dataset}");
        std::fs::remove_file(compressed_mongo_dataset).expect("Failed to remove tar file");
    }

    async fn download_dump_file(&self) -> String {
        let s3_download_file = format!("mongo-{}.tar.gz", self.database_name);
        let local_dump_file_path = format!("/tmp/mongo-dump/{}", s3_download_file);
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

        let s3_bucket_key = format!("{s3_bucket_key}/{s3_download_file}");

        info!("Downloading file {s3_download_file} from S3 bucket: {s3_bucket_name}, key: {s3_bucket_key}");

        Self::download_s3_file(
            s3_bucket_name,
            s3_bucket_key.as_str(),
            local_dump_file_path.as_str(),
        )
        .await;
        local_dump_file_path
    }

    async fn download_s3_file(bucket_name: &str, bucket_key: &str, local_file_path: &str) {
        let config = aws_config::defaults(BehaviorVersion::latest()).load().await;
        let client = Client::new(&config);

        let mut object = client
            .get_object()
            .bucket(bucket_name)
            .key(bucket_key)
            .send()
            .await
            .expect("Failed to download file from S3");

        let mut file = File::create(local_file_path).unwrap();

        while let Some(bytes) = object.body.try_next().await.unwrap() {
            file.write_all(&bytes).unwrap();
        }
    }

    async fn untar_file(
        compressed_mongo_dataset: &str,
        extracted_mongo_files_location: impl Into<String>,
    ) {
        let untar_command = format!(
            "tar -xf {compressed_mongo_dataset} -C {}",
            extracted_mongo_files_location.into()
        );

        ShellCommandExecutor::execute_cmd(&untar_command).await;
    }

    async fn execute_mongo_restore(&self, mongo_data_folder: impl Into<String>) {
        let ns_to = self
            .mongo_uri
            .split('/')
            .last()
            .unwrap()
            .split('?')
            .next()
            .unwrap();
        let dir = format!("{}/{}/", mongo_data_folder.into(), self.database_name);

        let mongo_restore_commands = [
            String::from("mongorestore"),
            format!("--uri={}", self.mongo_uri),
            format!("--dir={}", dir),
            format!("--nsFrom={}.*", self.database_name),
            format!("--nsTo={ns_to}.*"),
            String::from("--drop"),
            String::from("--gzip"),
        ];

        let mongo_restore_command = mongo_restore_commands.join(" ");
        ShellCommandExecutor::execute_cmd(&mongo_restore_command).await;
    }
}
