.PHONY: lint test build check schema-gen validate-schema docs graphify protos smoke infer-smoke help

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

infer-smoke:
	bash scripts/infer-smoke.sh

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
	@echo "infer-smoke      Phase-3 end-to-end batch-inference smoke (requires ROLLOUT_VLLM_AVAILABLE=1)"
