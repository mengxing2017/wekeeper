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

FROM caddy:2.10-alpine AS runtime
COPY frontend/Caddyfile /etc/caddy/Caddyfile
COPY --from=builder /workspace/frontend/apps/web/dist /srv
EXPOSE 80 443
