# OUT-007 — Commit and publish failure handling

**Status:** Implemented; clean-database rollback, lease-ownership, and publish-completion verification passed  
**Decision date:** 2026-08-09  
**Depends on:** [OUT-004 lease-backed worker claim](OUT-004-worker-claim.md), [OUT-005 deterministic retry and backoff](OUT-005-retry-backoff.md), [OUT-006 poison-message handling](OUT-006-poison-message-handling.md)

## Decision

Outbox insertion remains inside the same transaction as the canonical event,
audit, and terminal command result. A transaction rollback leaves none of
those rows committed. The outbox worker does not treat a claim as a successful
publish.

After the transport acknowledges a message, only the worker that owns an
active processing lease may call the publish-completion boundary. That
transition locks the row, validates the worker and lease expiry, sets
`delivery_status = published`, records `published_at`, and clears the claim.
The caller controls the transaction commit. A wrong worker or an expired lease
is rejected, and a rolled-back completion leaves the processing claim intact
for safe expiry recovery. Retry and poison transitions remain the explicit
paths for failed publication.

## Verification

The clean PostgreSQL `outbox_publish` fixture applied migrations 1–13 to a
dedicated project test database. It verified that rolling back a transaction
containing a command, canonical event, and outbox row leaves all three absent;
wrong-worker and expired-worker acknowledgements are rejected; the expired
claim is reclaimed with attempt count `2`; a successful completion persists
`published_at` and clears the lease; and a completion rollback does not persist
the published state.

The `outbox_retry`, `outbox_claim`, and `application_c1_execution` clean
fixtures were rerun against migrations 1–13. Workspace test, clippy, and
formatting gates passed after the change.
