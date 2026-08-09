# CMD-007 — Persist terminal command results

**Status:** Terminal-result schema and infrastructure/application read-write boundary implemented; clean-database C1/correction verification passed  
**Decision date:** 2026-08-09  
**Depends on:** [CMD-003 short command admission](CMD-003-short-command-admission.md), [CMD-004 idempotency fingerprint conflict](CMD-004-idempotency-fingerprint-conflict.md), [MCH-005 command result and domain event envelopes](MCH-005-command-event-envelopes.md), and [ARC-008 command, event, and version semantics](ARC-008-command-event-version-semantics.md)

## Decision

Migration `0007_terminal_command_results.sql` extends `commands` with the
terminal result fields needed for deterministic command reconciliation:

```text
aggregate_id
completed_at
final_aggregate_version
event_count
result_payload JSONB
rejection_code
rejection_details JSONB
```

The database check constraint keeps the three lifecycle shapes explicit:

- `accepted` requires the aggregate ID, completion time, non-negative final
  version, non-negative event count, and a JSON result payload;
- `rejected` requires the aggregate ID, completion time, non-negative observed
  version, a stable rejection code, and object-shaped safe details;
- `received`, `processing`, and `failed_retryable` must not carry terminal
  result data.

`persist_terminal_command_result` runs inside the caller-owned PostgreSQL
transaction. It finalizes only a row that is still `processing`, owned by the
supplied processing owner, and whose lease has not expired at the supplied UTC
completion time. It clears the processing lease in the same update. Accepted
payloads remain opaque JSON at this adapter boundary; the application owns the
typed command-result encoding. Rejected results accept only stable `ErrorCode`
values and JSON-object details.

Before that guarded update, the writer reads canonical event evidence in the
same transaction. It compares command-local event count and maximum version,
the command-local zero-based event-index range, the command's aggregate
identity, and the aggregate's current maximum version with the terminal result
metadata. An accepted result with events must match the command-local count,
contiguous event indexes, and final version; an accepted result with zero
events must not be ahead of the aggregate version visible in the transaction;
a rejected result must have no command-local events and must not claim a version
ahead of the observed aggregate. Later commands may legitimately advance the
aggregate after this result commits. A mismatch fails the write before the
command row becomes terminal.

The application helper `into_persisted_command_result` moves a typed
`CommandResultEnvelope` into the persistence-facing accepted/rejected shape.
It preserves command ID, aggregate ID, completion time, final/observed
version, and event count; command-specific accepted payloads and rejection
code/details are supplied by explicit encoders. It performs no database write
and does not invent a rejection code or JSON schema.

The infrastructure `TerminalCommandWrite` bridge converts that application
shape into the guarded PostgreSQL writer. It parses rejection codes only
through the platform `ErrorCode` vocabulary; an unknown code fails before any
SQL update. It also rejects retryable dependency/rate-limit codes from the
terminal `REJECTED` shape, preserving the separate `FAILED_RETRYABLE` command
state semantics. `persist_application_command_result` then delegates the
validated value to `persist_terminal_command_result` while the caller retains
ownership of the transaction and processing lease.

`load_terminal_command_result` reads the same row back as a strict
`StoredTerminalCommandResult`. It preserves accepted/rejected payloads and
rejects missing fields, negative numeric values, non-terminal statuses, unknown
statuses, unknown or retryable rejection codes, and contradictory terminal
shapes instead of guessing.

The migration intentionally does not fabricate missing terminal data for rows
created by an earlier version. An existing terminal row without the required
result shape causes migration validation to fail and must be reconciled from
authoritative command history.

## Verification

Database-free infrastructure and application tests verify
accepted/rejected shape encoding, strict result loading, safe rejection-detail
validation, command-identity matching, and PostgreSQL `BIGINT` overflow
rejection. Database-free infrastructure tests also verify that terminal
metadata cannot pass without matching canonical-event evidence, including a
zero-based contiguous command-local event-index range. The command-admission
fixture finalized and loaded both accepted and rejected commands through the
same transaction boundary in a clean dedicated database. The C1 fixture also
attempted a mismatched accepted result before the valid terminal write and
asserted that the command remained `processing`. The correction fixture
finalized and loaded accepted terminal results for goal, void, and later live
action commands. Command-specific transport decoding and the full application
domain executor remain later increments.
