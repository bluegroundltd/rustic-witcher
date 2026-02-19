#!/bin/sh

rustic-mongo-buddy export \
    --mongo-uri ${MONGO_URI} \
    --s3-path ${S3_PATH} \
    --database-name ${DATABASE} \
    --exclude-collections ${EXCLUDE_COLLECTIONS} \
    --include-collections ${INCLUDE_COLLECTIONS}
