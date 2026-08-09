# CMD-002 — Final rejection and retryable failure semantics

**Status:** Accepted  
**Decision date:** 2026-08-08  
**Depends on:** [CMD-001 logical command identity](CMD-001-logical-command-identity.md), [ARC-005 error taxonomy](ARC-005-error-taxonomy.md), [ARC-008 command, event, and version semantics](ARC-008-command-event-version-semantics.md)

## Decision

The logical command status set is:

```text
RECEIVED
PROCESSING
ACCEPTED
REJECTED
FAILED_RETRYABLE
```

Allowed status transitions are:

```text
RECEIVED → PROCESSING
PROCESSING → ACCEPTED
PROCESSING → REJECTED
PROCESSING → FAILED_RETRYABLE
FAILED_RETRYABLE → PROCESSING
```

`ACCEPTED` and `REJECTED` are terminal. A final domain rejection consumes the
client command identity and its safe rejection result is returned unchanged on
retry. It emits no canonical match event.

`FAILED_RETRYABLE` is recoverable only when the application has established
that no canonical event or critical aggregate commit occurred. An uncertain
commit is not blindly retried: the application must reconcile command and
event state by internal `CommandId` first. `FAILED_RETRYABLE → PROCESSING`
represents an explicit recovery/admission claim, not automatic duplicate
execution.

The infrastructure command adapter now exposes a guarded
`mark_command_failed_retryable` transaction update. It requires the active
processing owner and an unexpired lease, clears the lease on success, and
cannot be used to convert a transaction that may already contain canonical
state; that transaction must instead roll back or enter the reconciliation
path.

The read-only reconciliation snapshot validates the persisted lease shape and
derives recovery eligibility without mutating the command. A command with no
observed canonical event may be reclaimed from `RECEIVED` or
`FAILED_RETRYABLE`, or from expired `PROCESSING`; an active processing lease is
never treated as reclaimable.

The PostgreSQL admission boundary now applies the same no-blind-retry rule to
expired `PROCESSING`: it checks command-local canonical event evidence inside
the admission transaction before reclaiming the lease. Evidence causes
`idempotency.reconciliation_required` and preserves the processing row for
explicit reconciliation; only an expired command with no observed canonical
event is reclaimed.

This increment defines status vocabulary and transitions only. It does not
persist command rows, implement leases, compare fingerprints, map rejection
codes, or execute recovery.

## Verification

Database-free domain tests cover a retryable failure and recovery path to
acceptance, terminal status classification, and rejection of retries from
`REJECTED`.
