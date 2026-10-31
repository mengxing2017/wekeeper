#!/bin/sh
set -eu

cd "$(dirname "$0")/.."

compose() {
  docker compose --env-file "${WEKEEPER_COMPOSE_ENV_FILE:-.env}" -f compose.yaml "$@"
}

printf '%s\n' 'Resetting the dedicated Compose development database: all public schema data will be deleted.'
compose stop api web
compose up -d --wait postgres
compose exec -T postgres \
  sh -c 'exec psql -v ON_ERROR_STOP=1 --username "$POSTGRES_USER" --dbname "$POSTGRES_DB"' \
  < tools/reset-dev-schema.sql
compose up -d --wait api web
compose logs --tail 100 api
