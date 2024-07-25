#!/bin/bash

rustic-witcher anonymize \
    --bucket-name $S3_BUCKET_NAME \
    --s3-prefix $S3_PREFIX \
    --source-database-name $SOURCE_DATABASE \
    --target-application-users $TARGET_APPLICATION_USERS \
    --database-schema $SCHEMA \
    --included-tables-from-file $SOURCE_DATABASE-$SCHEMA/tables.txt \
    --mode full-load-only
