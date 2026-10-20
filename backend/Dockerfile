# syntax=docker/dockerfile:1
FROM rust:1.97.1-bookworm AS builder
WORKDIR /workspace/backend
COPY backend/Cargo.toml backend/Cargo.lock ./
COPY backend/src ./src
COPY backend/migrations ./migrations
RUN --mount=type=cache,id=wekeeper-cargo-registry,target=/usr/local/cargo/registry,sharing=locked \
    --mount=type=cache,id=wekeeper-cargo-target,target=/workspace/backend/target,sharing=locked \
    cargo build --locked --release \
    && cp target/release/wekeeper-api /tmp/wekeeper-api

FROM debian:bookworm-slim AS runtime
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates curl \
    && rm -rf /var/lib/apt/lists/* \
    && useradd --system --uid 10001 --no-create-home wekeeper
COPY --from=builder /tmp/wekeeper-api /usr/local/bin/wekeeper-api
USER 10001:10001
EXPOSE 8080
ENTRYPOINT ["/usr/local/bin/wekeeper-api"]
