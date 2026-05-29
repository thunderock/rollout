# GCS bucket setup for rollout-cloud-gcp

## Lifecycle policy (informational)

GCS retains incomplete resumable uploads for **7 days** automatically — no
lifecycle rule is required for orphan cleanup (D-SNAP-06 GCP variant). Because
`rollout-cloud-gcp` never persists a resumable session URL across processes
(Pitfall #5), a preempted worker simply re-uploads from byte 0; orphaned
sessions expire on their own.

To enforce earlier cleanup of the `temp/pending-*` staging objects, set a
short-age delete rule:

```bash
gsutil lifecycle set \
  <(echo '{"rule":[{"action":{"type":"Delete"},"condition":{"age":1,"matchesPrefix":["temp/"]}}]}') \
  gs://<bucket>
```

## IAM permissions

The Service Account / Workload-Identity-Federation principal running rollout needs:

- `roles/storage.objectAdmin` on the bucket (put / get / delete / copy for the
  content-addressed CAS layout and the temp-then-rename upload flow)
- `roles/pubsub.subscriber` + `roles/pubsub.publisher` on the topic/subscription
- `roles/secretmanager.secretAccessor` on each allowlisted secret
- `roles/compute.viewer` (for GCE MDS metadata reads — only when running on GCE/GKE)

## Key layout

Objects are content-addressed under `<prefix>cas/<ab>/<cd>/<hex>`, identical to
`FsObjectStore` (local) and `S3ObjectStore` (AWS), so snapshots are portable
across providers by content id.
