# COR-003 — Rebuild score after a voided goal

**Status:** Domain, codec, transaction, and clean-database acceptance verified on 2026-08-09  
**Decision date:** 2026-08-09  
**Depends on:** [COR-001 correction reason and immutable void event](COR-001-correction-reason-and-void-event.md), [COR-002 effective event-stream builder](COR-002-effective-event-stream-builder.md), and [PRJ-001 synchronous score and timeline reduction](PRJ-001-synchronous-score-timeline-reduction.md)

## Decision

Live C1 payloads and correction payloads share the canonical aggregate through
`MatchEventPayload::{C1, Correction}`. This union is only a canonical-stream
transport inside the domain; it does not merge the distinct business meanings
of live actions and correction markers.

`MatchProjection::from_effective_stream` consumes the correction-resolved
effective view. It reduces only the remaining `C1` source events, so an
`EventVoided` marker and its target cannot contribute to score or timeline.
The projection keeps canonical synchronization metadata: source version gaps
are allowed in the effective view, versions must still increase strictly, and
the final projection version is the effective view's source
`canonical_version()`.

The canonical goal event and void marker remain immutable and queryable. The
projection is rebuilt from the effective view; no operator-provided total score
is accepted and no historical event is edited.

The canonical union now also has an infrastructure codec over the existing
`match_events` JSONB payload and `voids_event_id` reference. C1 payloads and
`EventVoided` payloads remain separate schemas: C1 events cannot carry
correction references, while `EventVoided` requires one non-self
`voids_event_id`, no `supersedes_event_id`, and an exact nonblank `reason`
field. The codec preserves the rest of the immutable envelope metadata.

## Domain API

The match engine now exports `MatchEventPayload` and provides:

- `C1LiveActionCommand::into_canonical_unsequenced_event`;
- `C1LiveActionCommand::append_to_canonical_stream`;
- `VoidEventCommand::into_canonical_unsequenced_event`;
- `VoidEventCommand::append_to_stream`;
- `MatchProjection::from_effective_stream`.

Infrastructure additionally provides `encode_match_event_envelope`,
`decode_match_event_envelope`, and typed canonical stream loaders for the
`C1`/`Correction` union. These helpers do not apply corrections, rewrite
canonical history, or invent a command-result payload.

The infrastructure `execute_void_event_in_transaction` adapter now composes
the processing-lease guard, typed union stream load, domain append, and
correction-event persistence in the caller-owned transaction. It returns the
uncommitted canonical source stream; terminal command result, audit/outbox,
and commit remain outside this narrow adapter.

`execute_c1_live_action_in_canonical_stream_transaction` uses the same typed
union for subsequent live actions, preserving the ability to continue a match
after a correction marker instead of falling back to a C1-only decoder.

The correction append boundary checks the expected aggregate version before
looking up the target event, rejects a target absent from the loaded canonical
stream, and then delegates sequencing to `CanonicalEventStream`. It does not
mutate the loaded source stream in place.

The existing `from_stream` and `reduce_event` APIs retain their contiguous
canonical C1 semantics. The new effective path accepts canonical version gaps
created by correction removal and rejects non-increasing versions or an
unsupported correction payload that reaches the projection.

## Verification

The database-free domain fixture appends a home goal, an away penalty goal, and
a reasoned void marker for the home goal. Rebuilding the effective projection
produces `0-1`, keeps only the away goal in the timeline, and reports canonical
version `3`. The canonical goal and void reference remain present in the source
stream. Infrastructure unit fixtures also round-trip C1 and `EventVoided`
envelopes and verify reference/reason invariants. The clean-DB-gated
`crates/infrastructure/tests/correction_execution.rs` fixture composes three
admitted commands: a goal at version `1`, its immutable void marker at version
`2`, and a later yellow card at version `3`. It persists each terminal result,
loads the mixed union, builds the effective view, and verifies that replay
removes the goal while retaining the later card and canonical version `3`.
`cargo test -p football-domain --lib` passes with 109 tests and
`cargo test -p football-infrastructure --lib` passes with 65 tests. The
PostgreSQL fixture remains ignored in ordinary workspace runs, but passed
against the explicitly approved dedicated `football_socket_test` database.
No existing database was modified.

The clean correction/live-action vertical slice now verifies correction append,
replay, and terminal command behavior. Full REST exposure remains a later
increment.
