# syntax=docker/dockerfile:1
FROM node:22-bookworm-slim AS builder
WORKDIR /workspace/frontend
RUN corepack enable
COPY frontend/package.json frontend/pnpm-lock.yaml frontend/pnpm-workspace.yaml frontend/.npmrc ./
COPY api-contract ../api-contract
RUN pnpm --dir ../api-contract install --frozen-lockfile \
    && pnpm --dir ../api-contract build \
    && pnpm install --frozen-lockfile
COPY frontend ./
RUN pnpm build

FROM caddy:2.10-alpine AS runtime
COPY frontend/Caddyfile /etc/caddy/Caddyfile
COPY --from=builder /workspace/frontend/dist /srv
EXPOSE 80 443
