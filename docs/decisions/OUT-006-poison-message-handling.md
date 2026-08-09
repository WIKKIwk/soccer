# OUT-006 — Poison-message handling

**Status:** Implemented; clean-database terminal poison and no-reclaim verification passed  
**Decision date:** 2026-08-09  
**Depends on:** [OUT-005 deterministic retry and backoff](OUT-005-retry-backoff.md), [ARC-005 error taxonomy](ARC-005-error-taxonomy.md)

## Decision

An outbox message is terminally poisoned after a publish failure on delivery
attempt `10` or later. The failure transition is performed only by the worker
that owns an active processing lease. It changes the row to `poisoned`, clears
the claim owner and expiry, records `poisoned_at`, and stores only the stable
reason `max_delivery_attempts_exceeded`.

`poisoned` is a terminal delivery state for the current worker contract:

- it is not selected by the pending or expired-processing claim query;
- it cannot be rescheduled through the transient failure boundary;
- it retains the canonical message payload and attempt count for supervisor
  inspection;
- operator replay or manual remediation is not silently inferred and remains
  a later operational contract.

The threshold is deliberately separate from domain command rejection. A
poisoned integration delivery does not alter the canonical match event,
command result, audit row, or source event identity.

## Verification

The clean PostgreSQL `outbox_retry` fixture applies migrations 1–13, moves a
message through retry scheduling, sets the test message at the threshold, and
verifies the committed `poisoned` state, attempt count `10`, stable reason,
cleared lease, and zero subsequent worker claims.
