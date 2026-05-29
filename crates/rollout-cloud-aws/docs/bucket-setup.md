# AWS S3 bucket setup for rollout-cloud-aws

## Required: AbortIncompleteMultipartUpload lifecycle policy

`S3ObjectStore::put_stream` uses multipart upload. `MultipartGuard::drop` does a
best-effort spawn-abort of in-flight multiparts, but `SIGKILL` and tokio
runtime-shutdown paths can still leak orphan multiparts. Apply this lifecycle
rule to bound the storage cost (D-SNAP-06):

```bash
aws s3api put-bucket-lifecycle-configuration --bucket <bucket> --lifecycle-configuration '{
  "Rules": [{
    "ID": "abort-incomplete-multipart",
    "Status": "Enabled",
    "Filter": {},
    "AbortIncompleteMultipartUpload": { "DaysAfterInitiation": 1 }
  }]
}'
```

Why: a 1-day lifecycle reclaims any multipart that was not completed or aborted,
so a crashed worker can never accumulate unbounded partial-upload storage.

## IAM permissions

Object operations on the bucket:

- `s3:PutObject`, `s3:GetObject`, `s3:HeadObject`, `s3:DeleteObject`
- `s3:CopyObject` (streaming put copies `temp/pending-<ulid>` to the final CAS key)

Multipart operations on the bucket:

- `s3:CreateMultipartUpload`, `s3:UploadPart`, `s3:CompleteMultipartUpload`
- `s3:AbortMultipartUpload`, `s3:ListMultipartUploads`

Bucket-level:

- `s3:ListBucket` (used by `rollout cloud doctor` in a later plan)

## Notes

- Keys use a sharded content-addressed layout `<prefix>cas/<ab>/<cd>/<hex>`,
  mirroring the local `FsObjectStore`.
- Credentials come from the standard AWS provider chain; `BehaviorVersion::latest()`
  means EC2 metadata access is IMDSv2-only.
