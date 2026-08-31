# APP-002 — Expected version, idempotency, and audit boundary

**Status:** Minimal command-audit schema, transactional outbox schema, and C1 transaction integration clean-database verified; critical-state/outbox delivery and audit hardening pending  
**Decision date:** 2026-08-08  
**Depends on:** [APP-001 load-execute-commit orchestration](APP-001-load-execute-commit-orchestration.md), [CMD-003 short command admission](CMD-003-short-command-admission.md), [CMD-004 idempotency fingerprint conflict](CMD-004-idempotency-fingerprint-conflict.md), [MCH-004 aggregate version semantics](MCH-004-aggregate-version-semantics.md)

## Decision

The application separates three responsibilities:

1. `CommandAdmissionPort` establishes idempotency ownership and returns a
   terminal result or a claimed command.
2. `CommandExecutionContext` carries the authenticated `ActorIdentity`, an
   optional typed `AggregateVersion` expected by the domain transaction, and
   the processing owner/lease expiry copied from admission. Aggregate-mutating
   commands must provide an expected version; explicit creation/no-current-
   version operations may use `None`. The orchestrator rejects changed lease
   metadata before invoking the execution port.
3. `CommandExecutionPort::execute_and_commit` owns the atomic transaction
   boundary after admission. Its adapter must verify the processing lease,
   load and execute the aggregate, append canonical events, update critical
   state, write audit and outbox records, and persist the terminal result in
   one commit.

Idempotency is not reimplemented in the executor. A terminal admission result
   never executes the domain command. A claimed command whose execution result
   has a different internal `CommandId` is rejected by the orchestrator as an
   identity-integrity failure.

The first audit schema is intentionally minimal and command-correlated:
`command_audit` uses the logical `command_id` as its one-row-per-command
identity and stores the authenticated actor, aggregate, action, outcome,
observed/final version, event count, optional C1 event ID, stable rejection
code, safe object details, and UTC creation time. It does not copy request
payloads or idempotency fingerprints. Later `AUD-001` work may harden actor
metadata and extend the audit history model without changing canonical event
semantics.

The infrastructure pre-execution lease adapter now provides the first
transaction step: it locks the command row and validates the processing owner
and expiry before the aggregate stream is loaded. The C1 executor composes
this guard with aggregate load, event writes, command audit, and terminal-result
persistence. Critical synchronous state, outbox, and later audit hardening
remain separate increments.

The lease guard and C1 transaction adapter now expose a stable application
error mapping at their infrastructure boundary. Expected-version conflicts
map to `conflict.expected_version`, invalid live-period actions map to
`domain.invalid_transition`, missing required expected-version context maps
to `validation.required_field`, and lease ownership failures map to
`conflict.operator_lease`. During canonical event append, the PostgreSQL
aggregate-order unique constraint is also narrowed to
`conflict.expected_version`; the event primary key and causation-command/event
index unique constraints map to `conflict.unique_constraint`. Unknown or other
database failures and malformed persisted state remain dependency/internal
errors without SQL or stored-value leakage. Accepted commands now also insert
one versioned transactional `match.event` outbox message keyed to the source
event; rejected commands do not create an outbox row.

## Verification

Database-free application tests cover terminal short-circuiting, successful
claim execution, admission and execution failures, expected-version/actor/
lease-context propagation, and command-identity/lease mismatch rejection. No
database or external transaction is run by these tests. The clean-DB
`application_c1_execution` fixture additionally verifies accepted and rejected
audit read-back, accepted outbox envelope read-back, rejected commands without
canonical events or outbox rows, terminal replay through the application
orchestrator, and rollback when audit persistence fails before outbox
insertion.

The command-admission regression coverage also verifies that a stored command
row whose database identity does not match the requested logical command is
rejected as an internal integrity failure without exposing storage details.
On 2026-08-14, the command-admission identity, application admission, and C1
execution checks passed with fresh PostgreSQL targets. Critical synchronous
state, outbox delivery, and later audit hardening remain open.
