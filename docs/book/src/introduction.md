# Introduction

rollout is a Rust-core reinforcement-learning framework for large language models.
It supports PPO, GRPO, DPO/IPO/KTO, SFT, and reward-model training across training,
batch inference, and online inference modes, with multi-node distribution from day
one. AWS and GCP are first-class infra targets; vLLM is the default inference
backend; plugins can be authored in Python or Rust.
