ARG RUST_VERSION=1.88.0
ARG IMAGE_NAME="public.ecr.aws/docker/library/rust:${RUST_VERSION}-slim-bookworm"

# Build the actual app
FROM $IMAGE_NAME AS builder
WORKDIR /app
RUN apt-get update && apt-get install -yqq libssl-dev pkg-config

COPY . .
# Needs to be set during build time (intentionally does not have a default)
ARG ANONYMIZATION_MODE

RUN --mount=type=bind,source=src,target=src \
    --mount=type=bind,source=Cargo.toml,target=Cargo.toml \
    --mount=type=bind,source=Cargo.lock,target=Cargo.lock \
    --mount=type=cache,target=/app/target/release/ \
    --mount=type=cache,target=/usr/local/cargo/git/db \
    --mount=type=cache,target=/usr/local/cargo/registry/ \
    cargo build --locked --release --bin rustic-witcher --features rustic-anonymization-operator/$ANONYMIZATION_MODE && \
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
