# python/

Python packages: PyO3 bindings to the Rust core, plus Python-native plugin packages and shared utilities.

## Layout

```
python/
├── rollout/                   The main PyPI package; PyO3 bindings + thin wrappers
│   ├── pyproject.toml
│   ├── rollout/
│   │   ├── __init__.py
│   │   ├── _native.pyi        Generated from PyO3 stubs (CI-checked)
│   │   ├── _config_stubs.pyi  Generated from Rust config types (CI-checked)
│   │   └── ...
│   └── tests/
│
├── rollout-plugins/           Base classes / SDK for Python plugin authors
│   ├── pyproject.toml
│   ├── rollout_plugins/
│   │   ├── __init__.py
│   │   ├── plugin.py          The Plugin ABC + helpers
│   │   ├── dependencies.py    Type stubs for PluginDependencies
│   │   └── ...
│   └── tests/
│
├── rollout-eval-mmlu/         One PyPI package per bundled eval
├── rollout-eval-ifeval/
├── rollout-eval-gsm8k/
│
└── tooling/
    ├── pyproject.toml         Local-only utilities (codegen, fixtures, etc.)
    └── ...
```

## Conventions

- **`pyproject.toml`** is the source of truth per package. We use `maturin` for packages that build PyO3 bindings, plain `hatchling` / `setuptools` for pure-Python.
- **Type stubs** are required for every public symbol.
- **`ruff`** for formatting + linting. 120-char lines.
- **`pytest`** for tests.
- **No top-level imports of `rollout._native`** in `__init__.py`; we lazy-import to keep import time low.
- **NumPy-style docstrings** on public functions.

## Building locally

```bash
# Build all PyO3-binding packages
uv sync          # or: pip install -e ./python/rollout

# Run all Python tests
uv run pytest python/
```

## Type-checking

```bash
uv run mypy python/rollout
```

`mypy` consumes the generated `_config_stubs.pyi` so type checks see the same shape Rust enforces.

## Plugin authoring

The shortest path to writing a Python plugin:

```python
from rollout_plugins import Plugin, PluginDependencies

class MyRewardPlugin(Plugin):
    kind = "reward-model"

    def validate_config(self, config: dict) -> None:
        ...

    def init(self, config: dict, deps: PluginDependencies) -> None:
        ...

    async def score(self, samples: list[dict]) -> list[float]:
        ...

def create_plugin() -> Plugin:
    return MyRewardPlugin()
```

Ship a `rollout-plugin.toml` manifest alongside, and your plugin is discoverable.

Full plugin authoring guide is in [`/docs/specs/03-plugin-system.md`](../docs/specs/03-plugin-system.md).

## State: pre-implementation

Currently empty. The Python packages get scaffolded once `rollout-py` (Phase 2) lands.
