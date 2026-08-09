# OUT-005 — Deterministic outbox retry and backoff

**Status:** Implemented; clean-database retry scheduling and due-time claim verification passed  
**Decision date:** 2026-08-09  
**Depends on:** [OUT-004 lease-backed worker claim](OUT-004-worker-claim.md), [ARC-004 UTC timezone rules](ARC-004-utc-timezone-rules.md)

## Decision

A transient publish failure returns the message from `processing` to
`pending`, clears the worker claim, preserves `attempt_count`, and stores a
future `next_attempt_at`. The retry transition is allowed only when the
supplied worker still owns the message and its lease is active at the failure
timestamp. A wrong worker or an expired lease cannot release or reschedule the
message.

The delay is deterministic exponential backoff without random jitter in this
foundation slice:

```text
attempt 1 -> 1 second
attempt 2 -> 2 seconds
attempt 3 -> 4 seconds
...
maximum  -> 300 seconds
```

The claim query selects a pending message only when
`next_attempt_at <= now`. Retry scheduling does not mark a message published
and does not commit the caller's transaction. At the configured threshold the
same failure boundary returns a poison disposition instead of a retry; the
terminal poison state is defined in `OUT-006`.

## Verification

The clean PostgreSQL `outbox_retry` fixture applies migrations 1–13. It
verifies that a wrong worker is rejected, an owned first failure schedules a
one-second retry, an early claim returns no message, a due claim increments
the attempt count, the second failure schedules a two-second retry, and the
threshold failure becomes terminally poisoned.
