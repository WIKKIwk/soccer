# EVT-007 — Duplicate and concurrent command tests

**Status:** Integration tests executed successfully on 2026-08-09  
**Decision date:** 2026-08-09  
**Depends on:** [CMD-003 short command admission](CMD-003-short-command-admission.md), [CMD-004 idempotency fingerprint conflict](CMD-004-idempotency-fingerprint-conflict.md), and [ARC-008 command/event/version semantics](ARC-008-command-event-version-semantics.md)

## Decision

The command-admission integration fixture covers two distinct safety
properties:

- a repeated client command ID follows the stored status and fingerprint path;
- two concurrent requests with the same client command ID and fingerprint
  produce exactly one `Claimed` result, while the other observes
  `CommandInProgress` for the same internal command ID.

The concurrent case uses two independent admission calls against the same
PostgreSQL pool. The row-level lock and unique client-command identity are
therefore exercised at the database boundary, not simulated in a unit fake.

The C1 execution fixture adds a second concurrency property with distinct
logical commands targeting the same match and the same expected aggregate
version. Each command runs in its own domain transaction. Exactly one may
persist aggregate version `1`; the other must resolve to the stable expected-
version conflict, whether it observes the committed version before append or
loses the canonical aggregate-order unique constraint during a concurrent
insert. The committed stream is read back and must contain exactly one event.

The fixture requires an explicitly approved clean `TEST_DATABASE_URL` and
`ALLOW_CLEAN_DATABASE_MIGRATION=1`. It remains ignored in ordinary workspace
tests so ordinary verification cannot mutate a database. The two admission
concurrency cases and the C1 expected-version concurrency case passed against
fresh dedicated project test databases.

## Verification

The command-admission and C1 concurrency tests passed at the PostgreSQL
boundary. Exactly one duplicate admission claimed the command, and exactly
one of two same-version C1 commands persisted aggregate version `1`; the
other resolved to the expected-version conflict. Default verification still
performs no database writes because these tests remain ignored.
