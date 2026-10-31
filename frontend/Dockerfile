# syntax=docker/dockerfile:1
FROM node:22-bookworm-slim AS builder
WORKDIR /workspace/frontend
RUN corepack enable
COPY frontend/package.json frontend/pnpm-lock.yaml frontend/pnpm-workspace.yaml frontend/.npmrc ./
COPY frontend/apps/web/package.json apps/web/package.json
COPY frontend/apps/miniapp/package.json apps/miniapp/package.json
COPY api-contract ../api-contract
RUN pnpm --dir ../api-contract install --frozen-lockfile \
    && pnpm --dir ../api-contract build \
    && pnpm --filter @wekeeper/web install --frozen-lockfile
COPY frontend/apps/web apps/web
RUN pnpm build:web

FROM node:22-bookworm-slim AS docs-builder
WORKDIR /workspace/docs
RUN corepack enable
COPY docs/package.json docs/pnpm-lock.yaml docs/pnpm-workspace.yaml docs/.npmrc ./
RUN pnpm install --frozen-lockfile
COPY docs/ ./
ARG WEKEEPER_BUILD_VERSION=开发构建
RUN WEKEEPER_BUILD_VERSION="$WEKEEPER_BUILD_VERSION" pnpm build

FROM caddy:2.10-alpine AS runtime
COPY frontend/Caddyfile /etc/caddy/Caddyfile
COPY --from=builder /workspace/frontend/apps/web/dist /srv
COPY --from=docs-builder /workspace/docs/.vitepress/dist /srv/docs
COPY --from=docs-builder /workspace/docs/.vitepress/caddy/docs.caddy /etc/caddy/docs.caddy
RUN caddy validate --config /etc/caddy/Caddyfile --adapter caddyfile
EXPOSE 80 443
