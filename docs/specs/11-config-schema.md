# Spec 11 — Configuration schema

This spec specifies how `rollout` handles configuration. The headline: **Rust types are the only authoritative schema.** JSON Schema, Python type stubs, CLI help, and editor completions are all generated from those types. No hand-written parallel schemas. Ever.

This document is short on length and long on insistence. The reason: parallel schemas in different languages are the source of an entire category of bugs we refuse to ship with.

## 1. The rule

Config is defined exactly once, in Rust, on types that derive:

```rust
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct MyConfig { ... }
```

From those types, the framework produces:

- The **runtime config parser** (via `serde`).
- The **JSON Schema** (via `schemars`).
- The **Python type stubs (.pyi)** (via a workspace codegen step that walks `schemars` output).
- The **CLI help text** (via `clap`'s `derive` macros that read the same types).
- The **editor completions** (any JSON-Schema-aware editor: vim's `coc`, VSCode, JetBrains).

**No alternative source of truth is permitted.** Adding a TypeScript types file or a Python dataclass that mirrors a Rust config type is a CI-blocked PR.

## 2. Why this matters

Three things drift when schemas are parallel:

1. **Validation drift.** Rust accepts a config that the Python stub rejects (or vice versa). Users hit "works on my machine" bugs that are actually "works on the language whose schema is freshest".
2. **Constraint drift.** A field becomes required in Rust but the Python type still has it as optional. Users write code that compiles in Python and fails 30 seconds into a run.
3. **Documentation drift.** The CLI help text says one thing; the README says another; the actual code does a third. We've all seen this and we are not doing it again.

The fix is structural: there is *one* source, everything else is generated, and CI fails the build if generated artifacts are out of date.

## 3. Workflow

Adding a new config field:

1. Add the field to the relevant Rust type, with derives.
2. `cargo test` locally. The codegen-drift test will fail because generated artifacts are stale.
3. Run `cargo xtask schema-gen`. This regenerates:
   - `schemas/rollout.schema.json`
   - `python/rollout/_config_stubs.pyi`
   - `docs/schema-reference.md`
4. Commit the regenerated files.
5. Push. CI re-runs codegen and asserts no diff vs committed files.

If CI sees a diff, the PR is rejected with a message: "Regenerate schemas with `cargo xtask schema-gen` and commit."

## 4. Anatomy of a config type

```rust
/// Top-level run configuration. The file (toml/yaml/json) deserializes into this.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RunConfig {
    /// Schema version. Currently 1. The framework refuses to load configs with a future version.
    #[schemars(range(min = 1, max = 1))]
    pub schema_version: u32,

    /// Free-form metadata about the run; persisted in storage but not used by the framework.
    #[serde(default)]
    pub run: RunMetadata,

    pub storage: StorageConfig,
    pub cloud:   CloudConfig,
    pub algorithm: AlgorithmConfig,

    #[serde(default)]
    pub snapshots: SnapshotPolicy,

    #[serde(default)]
    pub plugins: PluginRegistry,

    #[serde(default)]
    pub telemetry: TelemetryConfig,
}
```

Conventions:

- **`#[serde(deny_unknown_fields)]`** on every config struct. Unknown fields are user errors (typos), not future-compat hooks.
- **`#[serde(default)]`** on fields that have a sensible default; the default function lives next to the type.
- **`#[schemars(...)]`** for range constraints, regex patterns, format hints.
- **Doc comments are part of the schema.** They surface in JSON Schema descriptions, Python stubs, CLI help. Write them like API docs.

## 5. Tagged unions for variant configs

Algorithm-specific config uses `serde`'s tagged enum:

```rust
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AlgorithmConfig {
    Ppo  (PpoSettings),
    Grpo (GrpoSettings),
    Dpo  (DpoSettings),
    Sft  (SftSettings),
    Rm   (RmSettings),
}
```

In TOML:

```toml
[algorithm]
kind = "ppo"
# ... PpoSettings fields ...
```

JSON Schema understands the discriminator and produces accurate per-variant schemas.

## 6. Cross-field constraints

Some constraints can't be expressed in `schemars` (e.g., "if `algorithm.kind == ppo`, then `reference_policy` is required"). These are expressed in code via a `validate_cross_fields` method on the root config:

```rust
impl RunConfig {
    pub fn validate_cross_fields(&self) -> Result<(), Vec<ConfigViolation>> {
        let mut errs = Vec::new();
        if let AlgorithmConfig::Ppo(ppo) = &self.algorithm {
            if ppo.reference_policy.is_none() && ppo.ppo.kl_coef_init > 0.0 {
                errs.push(ConfigViolation::new(
                    "algorithm.ppo.reference_policy",
                    "required when kl_coef_init > 0",
                ));
            }
        }
        if errs.is_empty() { Ok(()) } else { Err(errs) }
    }
}
```

Called by `rollout validate` and `rollout plan`. Returns *all* violations at once (not first-failure) — users want to see the full list, not fix one and re-run.

## 7. Schema versioning

The top-level `schema_version` field is the wire-compat anchor:

- Bumping `schema_version` is a breaking change requiring a major version bump (post-1.0).
- The framework refuses to load a config whose `schema_version` is higher than its own.
- Migrations from older `schema_version` to newer are explicit, documented, and (post-1.0) tooling-supported via `rollout config migrate`.

Until 1.0 we maintain `schema_version = 1` and accept breaking changes within it. Once 1.0 ships, schema bumps follow semver.

## 8. Defaults policy

Defaults are stated **once**, in code, on the Rust type. Documentation references the code as the source of truth.

- A field that has a default is `#[serde(default)]`.
- The default function is named `defaults::<field>` and lives in a `defaults` module of the same crate.
- Defaults are pure functions; they cannot read env vars or files. Env-var-derived values are part of the runtime context, not config defaults.

## 9. Sensitive fields

Fields containing secrets are typed:

```rust
pub struct CloudAwsConfig {
    pub region: String,
    pub bucket: String,
    #[serde(default)]
    pub credentials: AwsCredentialsConfig,    // never a raw `String` for secrets
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "source", rename_all = "snake_case")]
pub enum AwsCredentialsConfig {
    Chain,                                    // default: AWS provider chain
    Env  { prefix: String },
    Secret { store: SecretStoreRef, key: String },
}
```

We **never** accept raw secrets in the config file. They reference a `SecretStore` (spec 06).

Pre-commit hook + CI lint detect anything in a config file that pattern-matches a secret (`sk_`, `AKIA`, etc.).

## 10. Test contract

- **Schema codegen drift:** CI runs `cargo xtask schema-gen` and asserts `git diff` is clean. A drift fails CI.
- **Schema completeness:** every public config struct must `derive(JsonSchema)`. A workspace lint enforces.
- **Schema validity:** the generated `rollout.schema.json` is validated against the JSON Schema meta-schema.
- **Python stubs:** `mypy` runs against `rollout` package using the generated stubs; failures fail CI.
- **Round-trip:** a sample of `RunConfig` instances are generated via `proptest`, serialized to TOML/JSON/YAML, parsed back, and compared. Asserts loss-free round-trip across all three formats.
- **Default stability:** a CI test asserts that defaults of every public config type haven't changed unexpectedly (changes require explicit acknowledgement to surface them in the changelog).

## 11. Tooling

`xtask` (workspace runner) provides:

- `cargo xtask schema-gen` — regenerate all derived artifacts.
- `cargo xtask schema-check` — verify nothing is stale (used by CI).
- `cargo xtask schema-doc` — generate `docs/schema-reference.md`.
- `cargo xtask config-migrate <old.toml> --to v<n>` — post-1.0; not in v1.

## 12. Open questions

- **CUE / Dhall:** alternative schema languages with richer constraint expressiveness. v1 sticks with Rust + schemars; revisit only if cross-field validation logic explodes in size.
- **JSON Schema 2020-12 vs draft 7:** schemars supports both; pick the one with the best editor tooling at release time (currently 2020-12).
- **Programmatic config builders in Python:** Python users sometimes want to build configs in code rather than via a file. v1 supports this via Pydantic-style classes generated from the Python stubs. Phase 12 hardens this surface.
