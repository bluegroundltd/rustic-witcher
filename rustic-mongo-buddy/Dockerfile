ARG RUST_VERSION=1.92.0
ARG IMAGE_NAME="public.ecr.aws/docker/library/rust:${RUST_VERSION}-slim-trixie"

# Build the actual app
FROM $IMAGE_NAME AS builder
WORKDIR /app

RUN apt-get update && apt-get install -y gcc libffi-dev libssl-dev pkg-config zstd
COPY . .
RUN --mount=type=bind,source=src,target=src \
    --mount=type=bind,source=Cargo.toml,target=Cargo.toml \
    --mount=type=bind,source=Cargo.lock,target=Cargo.lock \
    --mount=type=cache,target=/app/target/release/ \
    --mount=type=cache,target=/usr/local/cargo/git/db \
    --mount=type=cache,target=/usr/local/cargo/registry/ \
    cargo build --locked --release --bin rustic-mongo-buddy && \
    mkdir -p /bin/rustic/ && \
    cp ./target/release/rustic-mongo-buddy /bin/rustic/

# Build the runtime image
FROM $IMAGE_NAME AS runtime
ARG MONGO_TOOLS_VERSION=100.14.0
WORKDIR /app

# Install mongodb-database-tools and zstd for compression
RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates wget gnupg zstd \
    && wget -qO- https://www.mongodb.org/static/pgp/server-7.0.asc | gpg --dearmor -o /usr/share/keyrings/mongodb.gpg \
    && echo "deb [signed-by=/usr/share/keyrings/mongodb.gpg] https://repo.mongodb.org/apt/debian bookworm/mongodb-org/7.0 main" > /etc/apt/sources.list.d/mongodb.list \
    && apt-get update \
    && wget -q https://fastdl.mongodb.org/tools/db/mongodb-database-tools-ubuntu2204-arm64-${MONGO_TOOLS_VERSION}.deb \
    && dpkg -i mongodb-database-tools-ubuntu2204-arm64-${MONGO_TOOLS_VERSION}.deb || true \
    && apt-get install -f -y --no-install-recommends \
    && rm mongodb-database-tools-ubuntu2204-arm64-${MONGO_TOOLS_VERSION}.deb \
    && rm -rf /var/lib/apt/lists/* \
    && mongorestore --version

COPY --from=builder /bin/rustic/rustic-mongo-buddy /usr/local/bin/rustic-mongo-buddy
COPY --from=builder /app/rustic-mongo-buddy/scripts /app/scripts
