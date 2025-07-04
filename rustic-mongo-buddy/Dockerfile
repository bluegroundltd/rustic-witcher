ARG RUST_VERSION=1.88.0
ARG IMAGE_NAME="public.ecr.aws/docker/library/rust:${RUST_VERSION}-alpine"

# Build the actual app
FROM $IMAGE_NAME AS builder
WORKDIR /app
RUN apk update && apk add gcc musl-dev libffi-dev openssl-dev pkgconfig
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
WORKDIR /app
RUN apk update && apk add mongodb-tools
COPY --from=builder /bin/rustic/rustic-mongo-buddy /usr/local/bin/rustic-mongo-buddy
COPY --from=builder /app/rustic-mongo-buddy/scripts /app/scripts
