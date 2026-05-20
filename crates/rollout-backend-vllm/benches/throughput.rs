//! Placeholder criterion harness so `[[bench]]` resolves at workspace build time.
//! Real raw-vLLM-vs-rollout overhead bench lands in plan 03-05 (Wave 2).
#![allow(missing_docs)]

use criterion::{criterion_group, criterion_main, Criterion};

fn placeholder(c: &mut Criterion) {
    c.bench_function("placeholder", |b| b.iter(|| 1 + 1));
}

criterion_group!(benches, placeholder);
criterion_main!(benches);
