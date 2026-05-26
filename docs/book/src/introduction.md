<div style="text-align: center; padding: 1rem 0 2rem;">
  <img src="./assets/logo.svg" alt="rollout" width="180" style="margin: 0 auto;"/>
</div>

# Introduction

> A high-performance, multi-node reinforcement-learning framework for large language models. Written in Rust. Pluggable in Python.

rollout is a Rust-core reinforcement-learning framework for large language models.
It supports PPO, GRPO, DPO/IPO/KTO, SFT, and reward-model training across training,
batch inference, and online inference modes, with multi-node distribution from day
one. AWS and GCP are first-class infra targets; vLLM is the default inference
backend; plugins can be authored in Python or Rust.

| Layer | What it owns |
|---|---|
| **Algorithms** | SFT · RM · PPO · GRPO · DPO / IPO / KTO |
| **Substrate** | Coordinator + workers + plugin host (PyO3 / sidecar RPC) |
| **Storage / Cloud** | Embedded · Postgres · S3 · GCS — AWS/GCP behind a layered trait |

See [Architecture](./architecture.md) for the full layered breakdown.
