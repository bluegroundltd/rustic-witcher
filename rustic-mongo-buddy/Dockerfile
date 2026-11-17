ARG RUST_VERSION=1.97.1
ARG IMAGE_NAME="public.ecr.aws/docker/library/rust:${RUST_VERSION}-slim-trixie"

# Dependency caching with cargo-chef: dependencies are compiled in their own
# layer that only changes when Cargo.toml/Cargo.lock change, so a source-only
# change recompiles just the workspace crates. The layers are shared between
# CI runs through the registry build cache (see Makefile).
FROM $IMAGE_NAME AS chef
WORKDIR /app
RUN apt-get update && apt-get install -y gcc libffi-dev libssl-dev pkg-config zstd
RUN cargo install cargo-chef --locked --version 0.1.78

# Compute the dependency recipe (depends only on manifests and the lockfile)
FROM chef AS planner
COPY . .
RUN cargo chef prepare --recipe-path recipe.json

# Build the actual app
FROM chef AS builder

# Compile dependencies only (cached unless the recipe changes). Must use the
# same profile/bin as the final build or the cache is not reused.
COPY --from=planner /app/recipe.json recipe.json
RUN cargo chef cook --locked --release --recipe-path recipe.json --bin rustic-mongo-buddy

COPY . .
RUN cargo build --locked --release --bin rustic-mongo-buddy && \
    mkdir -p /bin/rustic/ && \
    cp ./target/release/rustic-mongo-buddy /bin/rustic/

# Build the runtime image
FROM $IMAGE_NAME AS runtime
ARG MONGO_TOOLS_VERSION=100.14.0
WORKDIR /app

# Install mongodb-database-tools and zstd for compression
# Direct .deb download avoids the MongoDB APT repo, whose SHA1-signed key is
# rejected by Debian Trixie (sqv policy changed 2026-02-01).
RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates wget zstd \
    && wget -q https://fastdl.mongodb.org/tools/db/mongodb-database-tools-ubuntu2204-arm64-${MONGO_TOOLS_VERSION}.deb \
    && dpkg -i mongodb-database-tools-ubuntu2204-arm64-${MONGO_TOOLS_VERSION}.deb || true \
    && apt-get install -f -y --no-install-recommends \
    && rm mongodb-database-tools-ubuntu2204-arm64-${MONGO_TOOLS_VERSION}.deb \
    && rm -rf /var/lib/apt/lists/* \
    && mongorestore --version

COPY --from=builder /bin/rustic/rustic-mongo-buddy /usr/local/bin/rustic-mongo-buddy
COPY --from=builder /app/rustic-mongo-buddy/scripts /app/scripts
