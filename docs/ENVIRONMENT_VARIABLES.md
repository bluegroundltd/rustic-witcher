# Environment Variables

The following environment variables can be used to configure the behavior of the application:

| Name   | Default Value   | Purpose   |
|------------|------------|------------|
| RECORD_REDUCTION_ENABLED | false | Whether to reduce the number of records or export all of them |
| RNG_SEED | 42 | The randomized seed for anonymization |
| NUM_OF_BUFFERS | 80 | Number of concurrent threads anonymizing Parquet files |
| DB_CONNECT_TIMEOUT | 180s | Timeout for database connection |
| DB_MAX_POOL_SIZE | 24 | The max pool size for database connections |
| DB_KEEP_ALIVES | false | Whether to keep alive database connections |
| SUPERUSER_URL | None | Target URL for the database that will contain anonymized data |
| CREATE_ROLE_AS_SUPERUSER | false | Whether to create `{SUPERUSER_USERNAME}` role as `superuser` in the target database |
| S3_VPC_ENDPOINT | None | S3 VPC Endpoint for connection to S3 bucket of DMS exports through dev VPC |
| S3_BUCKET_REGION | eu-west-1 | Region of Parquet files containing bucket |
| UPLOAD_ANONYMIZED_FILES | false | Whether to upload anonymized files to S3 bucket |
| ANONYMIZED_BUCKET | None | Name of the S3 bucket containing anonymized Parquet files |
| SKIP_VALIDATIONS | false | Whether to skip validations after data export |
