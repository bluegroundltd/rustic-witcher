#!/bin/bash

set -e

docker_image_tag=${DOCKER_IMAGE:-"bluegroundltd/rustic-witcher:latest"}

echo ${DOCKER_IMAGE}

b="\033[34m"
r="\033[0m"

echo -e "${b}Building & pushing Docker image...${r}"
docker buildx use default
docker buildx build -t ${docker_image_tag} --push . --build-arg ANONYMIZATION_MODE=bg_source

exit 0
