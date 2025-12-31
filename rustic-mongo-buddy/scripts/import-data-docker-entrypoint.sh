#!/bin/sh

rustic-mongo-buddy import \
    --mongo-uri ${MONGO_URI_IMPORT} \
    --s3-path ${S3_PATH} \
    --database-name ${DATABASE} \
    --override-destination-database-name ${DESTINATION_DATABASE:-""} \
    --num-parallel-collections ${NUM_PARALLEL_COLLECTIONS:-4} \
    --num-insertion-workers ${NUM_INSERTION_WORKERS:-1}
