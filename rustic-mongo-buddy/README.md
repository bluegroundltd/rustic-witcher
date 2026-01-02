# Environment Variables

The following environment variables can be used to configure the behavior of the application:

## Export

| Name   | Default Value   | Purpose   |
|------------|------------|------------|
| MONGO_URI | - | Mongo URI of the cluster that we will export data from |
| S3_PATH | - | S3 Path for the storage of the exported dataset |
| DATABASE | - | Database name to export |
| EXCLUDE_COLLECTIONS | [] | A list of collections to be excluded from the export |

## Import

| Name   | Default Value   | Purpose   |
|------------|------------|------------|
| MONGO_URI_IMPORT | - | Mongo URI of the cluster that we will import data to |
| S3_PATH | - | S3 Path for the storage of the exported dataset |
| DATABASE | - | Database name to import |
| DESTINATION_DATABASE | "" | Override the above database name on the destination cluster, if empty it will be ignored |
| NUM_PARALLEL_COLLECTIONS | 4 | Number of collections to restore in parallel |
| NUM_INSERTION_WORKERS | 1 | Number of insertion workers per collection |
