.PHONY: dev-up dev dev-down test-e2e

dev-up:
	docker compose up --detach --wait valkey

dev: dev-up
	cargo run

dev-down:
	docker compose down

test-e2e:
	bash scripts/test-e2e-performance.sh
