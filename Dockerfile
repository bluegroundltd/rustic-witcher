ARG RUST_VERSION=1.97.1
ARG IMAGE_NAME="public.ecr.aws/docker/library/rust:${RUST_VERSION}-slim-bookworm"

# Dependency caching with cargo-chef: dependencies are compiled in their own
# layer that only changes when Cargo.toml/Cargo.lock change, so a source-only
# change recompiles just the workspace crates. The layers are shared between
# CI runs through the registry build cache (see Makefile).
FROM $IMAGE_NAME AS chef
WORKDIR /app
RUN apt-get update && apt-get install -yqq libssl-dev pkg-config
RUN cargo install cargo-chef --locked --version 0.1.78

# Compute the dependency recipe (depends only on manifests and the lockfile)
FROM chef AS planner
COPY . .
RUN cargo chef prepare --recipe-path recipe.json

# Build the actual app
FROM chef AS builder
# Needs to be set during build time (intentionally does not have a default)
ARG ANONYMIZATION_MODE

# Compile dependencies only (cached unless the recipe changes). Must use the
# same profile/features as the final build or the cache is not reused.
COPY --from=planner /app/recipe.json recipe.json
RUN cargo chef cook --locked --release --recipe-path recipe.json \
    --bin rustic-witcher \
    --features rustic-anonymization-operator/$ANONYMIZATION_MODE

COPY . .
RUN cargo build --locked --release --bin rustic-witcher --features rustic-anonymization-operator/$ANONYMIZATION_MODE && \
    mkdir -p /bin/rustic/ && \
    cp ./target/release/rustic-witcher /bin/rustic/

# Build the runtime image
FROM $IMAGE_NAME AS runtime

WORKDIR /app

ARG POSTGRES_CLIENT_VERSION="postgresql-client"

RUN apt update && apt install -yqq curl ca-certificates \
    && install -d /usr/share/postgresql-common/pgdg \
    && curl -o /usr/share/postgresql-common/pgdg/apt.postgresql.org.asc --fail https://www.postgresql.org/media/keys/ACCC4CF8.asc \
    && sh -c 'echo "deb [signed-by=/usr/share/postgresql-common/pgdg/apt.postgresql.org.asc] https://apt.postgresql.org/pub/repos/apt bookworm-pgdg main" > /etc/apt/sources.list.d/pgdg.list' \
    && apt-get update && apt-get install -yqq $POSTGRES_CLIENT_VERSION

COPY --from=builder /bin/rustic/rustic-witcher /usr/local/bin/rustic-witcher
COPY --from=builder /app/scripts /app/scripts
COPY --from=builder /app/configuration_data /app/configuration_data

ENTRYPOINT ["/app/scripts/docker-entrypoint.sh"]
