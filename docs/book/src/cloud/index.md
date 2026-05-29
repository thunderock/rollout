# Cloud

The cloud layer lifts rollout's local substrate to real object stores and queues
(AWS S3 + SQS, GCP GCS + Pub/Sub) behind the same `rollout-core` traits. Algorithm
crates never see a cloud SDK — a hard dependency-direction lint plus two CI gates
(`public-api-cloud-leak`, `forbidden-patterns`) keep SDK types out of `rollout-core`'s
public API and keep raw metadata URLs / `shell=True` / `libc::fork` out of the tree.

- [Streaming + lease trait methods](./traits.md)
