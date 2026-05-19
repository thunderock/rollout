# Examples

This page is reserved for the v1 working-model recipe (SHIP-03 hardened).

v1 cannot ship without at least one end-to-end recipe (`make example` or
`cargo run --example`) that takes a real small open-weights model, runs SFT or
PPO, completes on commodity hardware, is exercised by nightly CI, and is
documented here. See [`AGENTS.md`](../../../../AGENTS.md) §9.4.

The recipe lands progressively: Phase 4 (SFT stub) → Phase 9 (real recipe) →
Phase 12 (polished docs).
