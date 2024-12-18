ECR_REGISTRY?=""

.PHONY: build_and_push_docker_image
build_and_push_docker_image:
	echo "\e[32;1mBuild and Push docker image\e[0m"
	./scripts/docker-push.sh

.PHONY: update_submodules_remote
update_submodules_remote:
	echo "\e[32;1mUpdate submodules\e[0m"
	git submodule update --init --recursive --remote

RUSTIC_WITCHER_DOCKER_IMAGE_TAG?=master
.PHONY: build_and_push_docker_image_bg
build_and_push_docker_image_bg:
	docker context use default
	docker buildx build \
	-t $(ECR_REGISTRY)/bluegroundltd/rustic-witcher:$(RUSTIC_WITCHER_DOCKER_IMAGE_TAG) \
	-f Dockerfile . \
	--build-arg ANONYMIZATION_MODE=bg_source \
	--build-arg POSTGRES_CLIENT_VERSION=postgresql-client-17 \
	--push \
	--cache-from $(ECR_REGISTRY)/bluegroundltd/rustic-witcher:master

RUSTIC_MONGO_BUDDY_DOCKER_IMAGE_TAG?=master
.PHONY: build_and_push_mongo_buddy_bg
build_and_push_mongo_buddy_bg:
	docker context use default
	docker buildx build \
	-t $(ECR_REGISTRY)/bluegroundltd/rustic-mongo-buddy:$(RUSTIC_MONGO_BUDDY_DOCKER_IMAGE_TAG) \
	-f Dockerfile.rustic-mongo-buddy . \
	--push \
	--cache-from $(ECR_REGISTRY)/bluegroundltd/rustic-mongo-buddy:master

.PHONY: build_rustic_witcher_open_source
build_rustic_witcher_open_source:
	docker context use default
	docker buildx build \
	-t rustic-witcher-os:master \
	-f Dockerfile . \
	--build-arg ANONYMIZATION_MODE=open_source \

.PHONY: build_mongo_buddy_open_source
build_mongo_buddy_open_source:
	docker context use default
	docker buildx build \
	-t rustic-mongo-buddy-os:master \
	-f Dockerfile.rustic-mongo-buddy .

.PHONY: build_and_move_clis
build_and_move_clis:
	cd rustic-local-data-importer-cli && \
	cargo build --locked --release --bin rustic-local-data-importer-cli && \
	cd $(CURDIR) && \
	cp target/release/rustic-local-data-importer-cli rustic-local-data-importer-cli/ && \
	cd rustic-config-generator-cli && \
	cargo build --locked --release --bin rustic-config-generator-cli && \
	cd $(CURDIR) && \
	cp target/release/rustic-config-generator-cli rustic-config-generator-cli/

# Define variables for directories
CONFIGURATION_DATA_DIR := configuration_data
INCLUSIONS_DIR := $(CONFIGURATION_DATA_DIR)/inclusions
SEQUENCES_DIR := $(CONFIGURATION_DATA_DIR)/sequences_fix
VALIDATIONS_DIR := $(CONFIGURATION_DATA_DIR)/validations

# Create configuration directories
.PHONY: create_configuration_dir
create_configuration_dir:
	@if [ ! -d "$(CONFIGURATION_DATA_DIR)" ]; then \
	echo "Directory $(CONFIGURATION_DATA_DIR) does not exist. Creating..."; \
	mkdir -p $(CONFIGURATION_DATA_DIR); \
    elif [ -d "$(CONFIGURATION_DATA_DIR)" ] && [ -z "$$(ls -A $(CONFIGURATION_DATA_DIR))" ]; then \
    echo "Directory $(CONFIGURATION_DATA_DIR) exists and is empty."; \
    else \
        echo "Directory $(CONFIGURATION_DATA_DIR) exists and is not empty."; \
    fi
	@echo "Creating subdirectories $(INCLUSIONS_DIR), $(SEQUENCES_DIR) & $(VALIDATIONS_DIR)..."
	@mkdir -p $(INCLUSIONS_DIR) $(SEQUENCES_DIR) $(VALIDATIONS_DIR)
	@echo "Subdirectories created."

# Create empty modules (for open source)
.PHONY: create_empty_modules
create_empty_modules:
	rm -rf rustic-bg-whole-table-transformator
	cargo new rustic-bg-whole-table-transformator --lib
	@echo "rustic-bg-whole-table-transformator module created."

	rm -rf rustic-local-data-importer-cli
	cargo new rustic-local-data-importer-cli --lib
	@echo "rustic-local-data-importer-cli  module created."

.PHONY: initialize_open_source
initialize_open_source: create_configuration_dir create_empty_modules

.PHONY: initialize_bg
initialize_bg:
	gh repo clone bluegroundltd/rustic-witcher -- --recurse-submodules

.PHONY: run_tests_open_source
run_tests_open_source:
	cargo nextest run --all --features rustic-anonymization-operator/open_source

.PHONY: run_tests_bg
run_tests_bg:
	cargo nextest run --all --features rustic-anonymization-operator/bg_source

.PHONY: run_all_tests
run_all_tests: run_tests_open_source run_tests_bg
