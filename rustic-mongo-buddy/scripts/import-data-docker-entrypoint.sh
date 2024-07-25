#!/bin/sh

rustic-mongo-buddy import \
    --mongo-uri ${MONGO_URI} \
    --s3-path ${S3_PATH} \
    --database-name ${DATABASE}
