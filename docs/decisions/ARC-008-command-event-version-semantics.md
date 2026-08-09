# ARC-008 — Command, event, and version semantics

**Status:** Accepted  
**Decision date:** 2026-08-08  
**Depends on:** [ARC-002 partial event sourcing](ARC-002-partial-event-sourcing.md), [ARC-005 error taxonomy](ARC-005-error-taxonomy.md), and [ARC-007 migration policy](ARC-007-forward-only-migrations.md)

This record freezes the command admission, idempotency, optimistic concurrency, canonical event, and atomic commit semantics from the roadmap.

## Logical command identity

A logical command represents one business intention, independent of HTTP, CLI, or future transport. Its persisted identity and metadata are:

```text
command_id
client_command_id UNIQUE
request_fingerprint
actor_id
aggregate_type
aggregate_id
command_type
expected_version
status
created_at
started_at
completed_at
processing_owner
processing_expires_at
final_aggregate_version
result_reference
rejection_code
```

`client_command_id` is the caller’s stable idempotency identity. The fingerprint is a SHA-256 digest of the canonicalized command operation and request payload, not of transport-specific JSON whitespace or field ordering.

## Command status

The only initial command statuses are:

```text
RECEIVED
PROCESSING
ACCEPTED
REJECTED
FAILED_RETRYABLE
```

Terminal statuses are `ACCEPTED` and `REJECTED`. `RECEIVED`, `PROCESSING`, and `FAILED_RETRYABLE` are recoverable states subject to ownership and lease rules.

## Idempotency rules

- Same `client_command_id` and same fingerprint returns the persisted result; it does not execute the domain command again.
- Same `client_command_id` and different fingerprint returns `idempotency.fingerprint_mismatch` and never changes aggregate state.
- A final domain rejection consumes the command ID. Retrying it returns the same rejection code and safe details.
- Infrastructure failure is retryable only when no canonical commit occurred.
- A stale `PROCESSING` lease may be claimed by a recovery attempt after its UTC expiry, subject to the command-row lock.
- If commit outcome is uncertain, the application queries command and event state by command ID before any new execution attempt.

Command attempts are optional transport-observability records. They are not canonical events and cannot establish that a business command was accepted.

## Admission transaction

Command admission is a short transaction that establishes ownership before football state is changed:

1. insert or load the command by `client_command_id`;
2. compare the request fingerprint;
3. return the stored terminal result for a matching command;
4. reject or report an active unexpired `PROCESSING` lease;
5. claim `RECEIVED`, `FAILED_RETRYABLE`, or expired `PROCESSING` state;
6. set `processing_owner` and `processing_expires_at`;
7. commit the admission transaction.

The lease owner is checked again before the domain transaction commits. A second worker cannot execute the same claimed command concurrently.

## Aggregate version semantics

Each aggregate command carries `expected_version` unless the command contract explicitly defines a creation operation. The application loads the current aggregate version and rejects a stale value with `conflict.expected_version`; it never silently overwrites a newer stream.

The first aggregate version is `0`. If the current version is `v` and an accepted command emits `N` canonical events, those events receive aggregate versions:

```text
v+1, v+2, ... v+N
```

The final command result stores `final_aggregate_version`. Event order is determined by aggregate version and command-local `event_index`, never by UUID or wall-clock ordering.

## Canonical event semantics

Every canonical event contains:

```text
event_id
match_id
aggregate_version
causation_command_id
event_index
event_type
event_schema_version
period
clock_ms
occurred_at
recorded_at
actor_id
verification_status
payload
supersedes_event_id
voids_event_id
```

Required uniqueness constraints are:

```text
UNIQUE (match_id, aggregate_version)
UNIQUE (causation_command_id, event_index)
```

`event_index` starts at zero for each command. Payloads are typed and validated before persistence and replay. Published events are append-only and immutable. Corrections and voids are new events that reference the affected event.

A command may emit zero, one, or multiple events. Zero-event acceptance is allowed only for an explicitly defined no-op/query-like command whose result is persisted; a state-changing C1 command cannot claim acceptance while silently producing no required canonical change. A final domain rejection emits no canonical match events.

## Domain transaction

After admission, the claimed command is executed in the domain transaction:

1. lock the command row;
2. verify `processing_owner` and lease;
3. lock/load the aggregate stream or current critical row;
4. validate `expected_version`;
5. execute the domain command;
6. assign `v+1 ... v+N` to emitted events;
7. append canonical events;
8. update critical synchronous state;
9. write the audit entry;
10. write outbox messages;
11. persist `ACCEPTED` and final version, or persist final `REJECTED` with its stable code;
12. commit the transaction.

Command acceptance, canonical events, critical state, audit, and outbox insertion commit atomically. No integration or WebSocket message is published before this commit succeeds.

## Recovery and failure rules

- A final domain rejection commits `REJECTED`, its stable code, required audit, and no canonical match event.
- A retryable infrastructure failure never persists `ACCEPTED` without a canonical commit.
- A worker crash after claim is recovered through lease expiry and command-state reconciliation.
- A database error after an uncertain commit is resolved by querying command/event state; blind replay is forbidden.
- Derived projections may lag or rebuild, but they cannot change the command result or canonical stream.

## Acceptance criteria

ARC-008 is complete when duplicate commands are safe, fingerprint conflicts are deterministic, stale versions are rejected, event versions/indexes are contiguous and unique, corrections remain immutable, and command/domain transaction boundaries preserve atomic acceptance.

