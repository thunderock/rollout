.PHONY: lint test build check schema-gen validate-schema docs graphify protos smoke smoke-3node-aws smoke-3node-gcp infer-smoke postgres-test train-smoke help

export CARGO_TERM_COLOR := always

lint:
	cargo fmt --all -- --check
	cargo clippy --all-targets --all-features -- -D warnings

test:
	cargo test --workspace --tests

build:
	cargo build --workspace

check: lint test

schema-gen:
	cargo xtask schema-gen

validate-schema:
	cargo run -p rollout-cli -- schema --format json > /tmp/rollout-schema-test.json
	check-jsonschema --check-metaschema /tmp/rollout-schema-test.json

docs:
	mdbook build docs/book
	cargo doc --workspace --no-deps --all-features

graphify:
	npx graphify-ts generate . --directed --svg

protos:
	cargo xtask gen-protos

smoke:
	bash scripts/smoke.sh

smoke-3node-aws:
	bash scripts/smoke-3node.sh aws

smoke-3node-gcp:
	bash scripts/smoke-3node.sh gcp

infer-smoke:
	bash scripts/infer-smoke.sh

postgres-test:
	@docker info >/dev/null 2>&1 || { echo "Docker not running; start Docker and retry"; exit 1; }
	SQLX_OFFLINE=true cargo test -p rollout-storage --features postgres --test postgres_integration -- --include-ignored --test-threads=1
	SQLX_OFFLINE=true cargo test -p rollout-storage --features postgres --test postgres_lease -- --include-ignored --test-threads=1

train-smoke:
	bash scripts/train-smoke.sh

help:
	@echo "lint             cargo fmt --check + clippy -D warnings"
	@echo "test             cargo test --workspace --tests"
	@echo "build            cargo build --workspace"
	@echo "check            lint + test"
	@echo "schema-gen       regenerate schemas/rollout.schema.json + python stubs"
	@echo "validate-schema  meta-validate the JSON Schema (requires check-jsonschema)"
	@echo "docs             mdbook build + cargo doc --workspace --no-deps --all-features"
	@echo "graphify         build codebase knowledge graph via graphify-ts (out: graphify-out/)"
	@echo "protos           regenerate python/rollout/_proto/ stubs (requires grpcio-tools; opt-in)"
	@echo "smoke            end-to-end Phase-2 substrate test (1 coord + 2 workers + plugins; kills w1; asserts deadline detection)"
	@echo "smoke-3node-aws  Phase-6 1 coord + 3 workers (mock backend, no GPU); dispatch+steal; run done < 30s (aws variant)"
	@echo "smoke-3node-gcp  Phase-6 1 coord + 3 workers (mock backend, no GPU); dispatch+steal; run done < 30s (gcp variant)"
	@echo "infer-smoke      Phase-3 end-to-end batch-inference smoke (requires ROLLOUT_VLLM_AVAILABLE=1)"
	@echo "postgres-test    Phase-4 testcontainers Postgres integration tests (requires Docker)"
	@echo "train-smoke      Phase-4 end-to-end SFT train smoke (requires ROLLOUT_TRANSFORMERS_AVAILABLE=1)"
