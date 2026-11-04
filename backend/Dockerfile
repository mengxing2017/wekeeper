# syntax=docker/dockerfile:1
FROM rust:1.97.1-bookworm AS builder
RUN if [ -f /etc/apt/sources.list ]; then \
      sed -i 's/deb.debian.org/mirrors.ustc.edu.cn/g' /etc/apt/sources.list; \
    fi \
    && if [ -f /etc/apt/sources.list.d/debian.sources ]; then \
      sed -i 's/deb.debian.org/mirrors.ustc.edu.cn/g' /etc/apt/sources.list.d/debian.sources; \
    fi \
    && apt-get update \
    && apt-get install -y --no-install-recommends cmake pkg-config git ca-certificates \
    && rm -rf /var/lib/apt/lists/*
# Use system Git instead of Cargo libgit2 for pinned Git dependencies.
ENV CARGO_NET_GIT_FETCH_WITH_CLI=true
WORKDIR /workspace/backend
COPY backend/Cargo.toml backend/Cargo.lock ./
COPY backend/src ./src
COPY backend/migrations ./migrations
RUN --mount=type=cache,id=wekeeper-cargo-registry,target=/usr/local/cargo/registry,sharing=locked \
    --mount=type=cache,id=wekeeper-cargo-git,target=/usr/local/cargo/git,sharing=locked \
    --mount=type=cache,id=wekeeper-cargo-target,target=/workspace/backend/target,sharing=locked \
    cargo fetch --locked --verbose \
    && cargo build --frozen --release \
    && cp target/release/wekeeper-api /tmp/wekeeper-api \
    && cp target/release/mobile-capability /tmp/mobile-capability \
    && cp target/release/mobile-runtime-config /tmp/mobile-runtime-config

FROM debian:bookworm-slim AS runtime
RUN if [ -f /etc/apt/sources.list ]; then \
      sed -i 's/deb.debian.org/mirrors.ustc.edu.cn/g' /etc/apt/sources.list; \
    fi \
    && if [ -f /etc/apt/sources.list.d/debian.sources ]; then \
      sed -i 's/deb.debian.org/mirrors.ustc.edu.cn/g' /etc/apt/sources.list.d/debian.sources; \
    fi \
    && apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates curl \
    && rm -rf /var/lib/apt/lists/* \
    && useradd --system --uid 10001 --no-create-home wekeeper
COPY --from=builder /tmp/wekeeper-api /usr/local/bin/wekeeper-api
COPY --from=builder /tmp/mobile-capability /usr/local/bin/mobile-capability
COPY --from=builder /tmp/mobile-runtime-config /usr/local/bin/mobile-runtime-config
COPY backend/runtime-resources /opt/wekeeper/runtime-resources
USER 10001:10001
EXPOSE 8080
ENTRYPOINT ["/usr/local/bin/wekeeper-api"]
