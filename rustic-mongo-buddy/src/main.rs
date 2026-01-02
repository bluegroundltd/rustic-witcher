use clap::{Parser, Subcommand};
use mongo_data_exporter::MongoDataExporter;
use mongo_data_importer::MongoDataImporter;
use tracing::info;

mod mongo_data_exporter;
mod mongo_data_importer;

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
#[command(propagate_version = true)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    Import {
        #[arg(long, required = true)]
        mongo_uri: String,
        #[arg(long, required = true)]
        s3_path: String,
        #[arg(long, required = true)]
        database_name: String,
        #[arg(long, required = false, default_value_t = String::from(""))]
        override_destination_database_name: String,
        /// Number of collections to restore in parallel (default: 4)
        #[arg(long, required = false, default_value_t = 4)]
        num_parallel_collections: u32,
        /// Number of insertion workers per collection (default: 1)
        #[arg(long, required = false, default_value_t = 1)]
        num_insertion_workers: u32,
    },
    Export {
        #[arg(long, required = true)]
        mongo_uri: String,
        #[arg(long, required = true)]
        s3_path: String,
        #[arg(long, required = true)]
        database_name: String,
        #[arg(long, value_delimiter = ',', num_args = 0.., required = false)]
        exclude_collections: Vec<String>,
    },
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();

    let cli = Cli::parse();
    match cli.command {
        Commands::Import {
            mongo_uri,
            s3_path,
            database_name,
            override_destination_database_name,
            num_parallel_collections,
            num_insertion_workers,
        } => {
            let mongo_host = mongo_uri.split('@').collect::<Vec<_>>()[1];

            info!(
                "Downloading data from {} to {} in {}",
                s3_path, mongo_host, database_name
            );
            let mongo_data_importer = MongoDataImporter::new(
                mongo_uri,
                s3_path,
                database_name,
                override_destination_database_name,
                num_parallel_collections,
                num_insertion_workers,
            );
            mongo_data_importer.import_data().await;
        }
        Commands::Export {
            mongo_uri,
            s3_path,
            database_name,
            exclude_collections,
        } => {
            let mongo_host = mongo_uri.split('@').collect::<Vec<_>>()[1];

            info!(
                "Exporting data from {} to {} in {} excluding {:?}",
                mongo_host, s3_path, database_name, exclude_collections
            );
            let mongo_data_exporter =
                MongoDataExporter::new(mongo_uri, s3_path, database_name, exclude_collections);
            mongo_data_exporter.export_data().await;
        }
    }
}
