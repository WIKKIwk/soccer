# APP-001 — Load-execute-commit command orchestration

**Status:** C1 application execution slice (canonical event, audit, transactional outbox, and terminal result) clean-database verified; critical-state executor and outbox delivery pending  
**Decision date:** 2026-08-08  
**Depends on:** [CMD-003 short command admission](CMD-003-short-command-admission.md), [CMD-004 idempotency fingerprint conflict](CMD-004-idempotency-fingerprint-conflict.md), [MCH-005 command result and event envelopes](MCH-005-command-event-envelopes.md), [ARC-008 command, event, and version semantics](ARC-008-command-event-version-semantics.md)

## Decision

The application owns the `CommandAdmissionPort` and
`CommandAdmissionResult` types. The port establishes command ownership before
any aggregate mutation and does not expose SQLx or PostgreSQL types to the
application crate.

`CommandAdmissionResult::ExistingTerminal` now carries a
`PersistedCommandResult`, including the accepted/rejected metadata and opaque
JSON payload needed for idempotent replay. The application validates that this
result belongs to the requested internal `CommandId` before returning it and
never sends an existing terminal result to the execution port.

The PostgreSQL adapter implements the port through the local
`PostgresCommandAdmission` wrapper. A wrapper is required because Rust’s orphan
rules do not allow infrastructure to implement an application trait directly
for the external `sqlx::PgPool` type.

The infrastructure `verify_processing_lease` boundary locks the claimed
command row with `FOR UPDATE` inside the later domain transaction and rejects
missing, non-processing, malformed, expired, or differently owned leases
before domain execution. The terminal-result update retains its separate
final lease guard at commit time.

The narrow `execute_c1_live_action_in_transaction` adapter now composes that
guard with transaction-local C1 stream load, domain expected-version append,
and typed event persistence. It returns the uncommitted stream to the caller;
terminal result, audit, outbox, and commit remain outside this narrow adapter.

The `execute_void_event_in_transaction` adapter applies the same lease and
context-integrity boundary to the typed C1/correction union. It loads the
canonical source stream, requires the correction target to exist, appends the
immutable void marker, and persists it without committing the caller's
transaction. It does not invent a correction command-result payload.

The parallel `execute_c1_live_action_in_canonical_stream_transaction` adapter
loads the same union, so a later goal/card/substitution can continue after a
correction marker without attempting to decode the full stream as C1-only. The
older C1-only adapter remains available for streams whose persisted contract
contains only C1 events.

`PostgresC1LiveActionExecutor` now implements the application
`CommandExecutionPort` for this current C1 slice. After admission it begins a
caller-independent PostgreSQL transaction, validates the processing lease,
loads and appends the canonical C1/correction-union event, persists the typed
command audit, inserts one versioned `match.event` outbox message for the
accepted source event, persists the terminal result, and commits. A deterministic
expected-version conflict is returned as a `REJECTED` terminal result in the
same transaction and writes no canonical event. The accepted result payload
currently carries the appended event ID and event type. The audit row stores
only command/actor/aggregate outcome metadata and safe rejection details; it
does not copy request payloads or fingerprints. Critical synchronous state and
outbox delivery/worker state remain later contracts.

The event repository also exposes raw and typed C1 event lookup by internal
`CommandId`, both through a pool and through a caller-owned transaction. This
lookup is explicitly reconciliation evidence for an uncertain commit and
preserves command-local event ordering; it does not masquerade as a complete
aggregate stream or permit blind replay.

The read-only reconciliation adapter now captures command status, terminal
result and command-local event evidence, and reads the aggregate's maximum
persisted version in that repeatable-read snapshot. It
classifies a consistent accepted result, a consistent rejected result, a
non-terminal command with no observed canonical events, or an inconsistent
state that requires reconciliation. It never claims that a non-terminal
no-event snapshot is safe to retry by itself. Only the inconsistent disposition
maps to the stable `idempotency.reconciliation_required` application code;
consistent observations remain state evidence rather than errors.

Failures while loading the snapshot map separately: database availability
failures use `dependency.database_unavailable`, a missing command uses
`resource.not_found`, and malformed or contradictory persisted terminal/event
evidence uses `internal.unexpected`. SQL details and malformed stored values
are not exposed through this application boundary.

The snapshot also validates the persisted processing-lease shape and exposes a
read-only recovery decision: `RECEIVED` and `FAILED_RETRYABLE` commands with no
canonical events are reclaimable, while `PROCESSING` is reclaimable only after
its lease expires. An active lease and all terminal/inconsistent states remain
ineligible; the decision does not perform the admission reclaim update.

After admission, `CommandExecutionContext` carries the internal command ID,
authenticated actor identity, optional expected aggregate version, and the
processing owner/lease expiry into the execution port. The lease metadata is
copied from the admission request and the orchestrator rejects a claimed
execution context that changes it. It also rejects a context whose command ID
differs from the admitted request and a terminal result whose command ID does
not match the admitted command.

The eventual application flow is:

1. validate and build the logical command request;
2. call the admission port;
3. return a matching terminal result without executing the domain command;
4. pass a claimed command to the domain executor;
5. commit canonical events, critical state, audit, outbox, and terminal result
   atomically; or
6. persist a final rejection without canonical match events.

The typed ports and the database-independent orchestration of steps 1–4 are
prepared in this increment. The C1 application slice now composes the event
store, command audit, versioned transactional outbox insertion, and
terminal-result persistence, while the complete production executor, critical
synchronous state, outbox delivery worker, and transport handlers remain later
increments because those contracts are not yet implemented. The
clean-DB-gated correction fixture proves the three transactional event-store
slices can be composed in order without making the terminal result an untyped
substitute for a future command-specific contract.

The application `into_persisted_command_result` helper now closes the typed
domain-result to persistence-result conversion boundary without selecting a
command-specific payload or rejection code. The C1 infrastructure transaction
adapter can use this result after its event append, while the caller still
owns terminal persistence and commit ordering.

## Verification

Application and infrastructure tests verify terminal-result preservation,
command-identity matching, the no-execution terminal short circuit, and the
Postgres admission adapter through the application orchestrator. The clean-DB
`application_admission` fixture additionally proves that a matching terminal
retry returns the persisted accepted result and does not invoke the execution
port, while a fingerprint conflict stops at admission and leaves one command
row.
Database-free command, lease propagation, lease-mismatch, and corrupted
command-local event-index reconciliation tests pass. Database-free correction
adapter tests also cover context validation, target-not-found mapping, and
expected-version conflict mapping. The clean-DB C1 fixture covered admission,
same-transaction load/append, pre-commit readback, terminal-result persistence,
commit, post-commit event replay through `MatchProjection`, reconciliation, and
terminal-result readback. The correction fixture additionally verified the
mixed canonical stream path. The clean-DB
`application_c1_execution` fixture verifies the application orchestrator's
accepted C1 commit, accepted audit, versioned outbox readback, projection
replay, deterministic rejected command with rejected audit and no outbox row,
terminal replay, and rollback without an outbox row when audit persistence
fails. The full critical-state, outbox delivery worker, and transport handlers
remain pending.

On 2026-08-14, the clean PostgreSQL admission fixtures were rerun with one
fresh target per test and both passed; the C1 execution fixture also passed on
a fresh target. The application slice therefore has current repeatable
acceptance evidence without claiming the later critical-state or delivery
contracts.
