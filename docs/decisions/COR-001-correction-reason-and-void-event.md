# COR-001 — Correction reason and immutable void event

**Status:** Implemented and clean-database verified on 2026-08-09  
**Decision date:** 2026-08-09  
**Depends on:** [P0-006 initial C1 commands and events](P0-006-c1-commands-events.md), [MCH-005 command/event envelopes](MCH-005-command-event-envelopes.md), and [ARC-002 partial event sourcing](ARC-002-partial-event-sourcing.md)

## Decision

Every event correction or void carries a typed `CorrectionReason`. The reason
must contain at least one non-whitespace character and is preserved exactly;
the domain does not trim or truncate evidence. Transport body limits and any
authorization policy remain application concerns.

The first correction event is `EventVoided`, schema version `1`. Its typed
payload contains the reason. The affected immutable event ID is carried once
in the canonical envelope's `voids_event_id` field. The original event is not
updated or deleted.

`VoidEventCommand` carries the command/event identity, match identity,
expected aggregate version, football timing metadata, actor, verification
status, target event ID, and typed reason. It builds an unsequenced
`DomainEventEnvelope<CorrectionEventPayload>`; the canonical stream remains
responsible for assigning the aggregate version and command-local event index.
Its union-stream append helper checks the expected version, requires the target
to exist in the loaded source stream, and then delegates sequencing to the
canonical stream. A command cannot emit an event that voids itself.

This increment intentionally does not implement the effective event-stream
builder, correction-chain cycle detection, score rebuilding, authorization, or
transport adapter. Those are separate boundaries for `COR-002`, `COR-003`, and
later application/infrastructure work. The typed persistence codec is recorded
under `COR-003`; it does not change this domain decision. `EventCorrected`
replacement semantics are also not guessed here; the current increment only
freezes the reason and void contract explicitly required by `COR-001`.

## Domain exports

The match engine exports:

- `CorrectionReason` and `CorrectionReasonError`;
- `CorrectionEventType` and `CorrectionEventPayload`;
- `EventVoidedPayload`;
- `VoidEventCommand` and `VoidEventCommandError`.

## Verification

Database-free tests cover blank-reason rejection, exact reason preservation,
stable event type/schema, envelope correction references and metadata,
self-void rejection, expected-version sequencing, and target existence.
The infrastructure correction adapter composes the same command with lease
validation, typed canonical loading, and immutable persistence inside the
caller-owned transaction. `cargo test -p football-domain --lib` passes with
109 tests. No database rows or external state are changed by default
verification.

The correction fixture verified the immutable void append, target reference,
terminal result, and post-commit typed canonical read. The remaining
application/API authorization workflow belongs to later increments.
