# OUT-004 — Lease-backed outbox worker claim

**Status:** Implemented; clean-database concurrent claim and expiry-recovery verification passed  
**Decision date:** 2026-08-09  
**Depends on:** [OUT-001 transactional outbox foundation](OUT-001-transactional-outbox-foundation.md), [ARC-007 forward-only migrations](ARC-007-forward-only-migrations.md)

## Decision

Outbox delivery claims live on the message row so one PostgreSQL transaction
owns both the selection lock and the lease write. Migration `0010` adds:

- `delivery_status`: `pending`, `processing`, or `published`;
- `attempt_count`;
- `claim_owner`;
- `claim_expires_at`.

The claim repository function requires a positive batch size and a future
lease expiry. In one caller-owned transaction it:

1. selects pending rows or processing rows whose lease has expired;
2. orders candidates by `created_at, message_id`;
3. locks candidates with `FOR UPDATE SKIP LOCKED`;
4. changes each selected row to `processing`, increments `attempt_count`, and
   assigns the worker and new expiry;
5. returns the claimed message metadata without committing the caller's
   transaction.

An active processing lease is not claimable by another worker. An expired
processing lease is the explicit recovery path for a worker that stopped
before completion. Publish acknowledgement and poison handling are
intentionally not inferred from a claim and remain later increments.

## Verification

The clean PostgreSQL `outbox_claim` fixture applies migrations 1–13. It holds
one worker transaction open while a second worker claims the remaining row,
proving `SKIP LOCKED` produces disjoint claims. It then expires one lease and
verifies a new worker reclaims that row with attempt count `2`; rows marked
`published` are not claimed again.
