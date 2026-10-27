.PHONY: install check check-web check-backend fmt test-unit test-integration test-embedded api-dev dev-embedded web-dev miniapp-dev check-miniapp compose-config compose-up compose-down smoke

install:
	pnpm --dir api-contract install --frozen-lockfile
	pnpm --dir frontend install --frozen-lockfile

check: check-web check-backend

check-web:
	pnpm --dir api-contract build
	pnpm --dir api-contract test
	pnpm --dir frontend typecheck
	pnpm --dir frontend lint
	pnpm --dir frontend test
	pnpm --dir frontend build

fmt:
	cargo fmt --manifest-path backend/Cargo.toml --all

check-backend:
	cargo fmt --manifest-path backend/Cargo.toml --all --check
	cargo build --locked --manifest-path backend/Cargo.toml
	cargo clippy --locked --manifest-path backend/Cargo.toml --all-targets -- -D warnings
	cargo test --locked --manifest-path backend/Cargo.toml

test-unit:
	cargo test --locked --manifest-path backend/Cargo.toml

test-integration:
	@test -n "$$WEKEEPER_TEST_DATABASE_URL" || (echo "Set isolated WEKEEPER_TEST_DATABASE_URL" >&2; exit 1)
	cargo test --locked --manifest-path backend/Cargo.toml --lib --test database --test business --test directory --test assets --test broadcast --test mobile_delivery -- --ignored

api-dev:
	cargo run --locked --manifest-path backend/Cargo.toml

dev-embedded:
	python3 tools/repair_rustup_linker.py
	exec cargo run --locked --manifest-path backend/Cargo.toml --example embedded_postgres

test-embedded:
	python3 tools/test_repair_rustup_linker.py
	python3 tools/repair_rustup_linker.py
	cargo build --locked --manifest-path backend/Cargo.toml --example embedded_postgres
	python3 tools/check_embedded.py

check-miniapp:
	pnpm --dir frontend --filter @wekeeper/miniapp typecheck
	pnpm --dir frontend --filter @wekeeper/miniapp lint
	pnpm --dir frontend --filter @wekeeper/miniapp test
	pnpm --dir frontend build:miniapp

miniapp-dev:
	pnpm --dir frontend dev:miniapp

web-dev:
	pnpm --dir frontend dev

compose-config:
	docker compose --env-file .env -f compose.yaml -f compose.dev.yaml config --quiet

compose-up:
	docker compose --env-file .env -f compose.yaml -f compose.dev.yaml up --build --wait

compose-down:
	docker compose --env-file .env -f compose.yaml -f compose.dev.yaml down

smoke:
	sh tools/smoke.sh

.PHONY: test-business test-migration
test-business:
	python3 tools/check_business.py

test-migration:
	python3 -B tools/migration/test_convert.py

.PHONY: test-live
test-live:
	cargo build --locked --manifest-path backend/Cargo.toml --example embedded_postgres
	python3 tools/check_live.py
