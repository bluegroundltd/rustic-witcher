# Local Execution

In order to check/debug if the transformation configurations are successful, you can locally execute rustic-witcher with specific flags limited to the tables of interest.

## Prerequisites

Before running tests, ensure you have the following installed:
- [Rust](https://www.rust-lang.org/)
- [Cargo](https://doc.rust-lang.org/cargo/)

## Execution

In order to run locally, you can create a script.sh file like the following:

```shell
#!/bin/sh

database="DATABASE_OF_INTEREST"
schema="SCHEMA_OF_INTEREST"
s3_prefix="X"
rds="X-read-replica"
user="X"
password="X"
datalake_bucket="X"
table_file="$database-$schema/tables.txt"


export ${DATABASE_OF_INTEREST}_${SCHEMA_OF_INTEREST}_SOURCE_POSTGRES_URL="postgres://$user:$password@$rds.X"
export ${DATABASE_OF_INTEREST}_${SCHEMA_OF_INTEREST}_TARGET_POSTGRES_URL="postgres://postgres:postgres@localhost:5438"

RUST_LOG=info \
    cargo run anonymize \
    --bucket-name $datalake_bucket \
    --s3-prefix $s3_prefix \
    --source-database-name $database \
    --database-schema $schema \
    --included-tables-from-file $table_file \
    --mode full-load-only

```

Things to keep in mind:
- Make sure you keep only the tables of interest in the `table_file` in order to make the debugging process faster
- Ensure that you use a RO user and connect to the respective `read replica` RDS

Then start docker-compose:
```shell
docker-compose up
```

Finally, execute the script:

```shell
chmod +x script.sh
./script.sh
```
