# EVT-004 — Append multiple events with expected version

**Status:** Implemented as a database-independent domain boundary  
**Decision date:** 2026-08-08  
**Depends on:** [EVT-003 load event stream](EVT-003-load-event-stream.md), [MCH-004 aggregate version semantics](MCH-004-aggregate-version-semantics.md), and [ARC-008 command/event version semantics](ARC-008-command-event-version-semantics.md)

## Decision

`CanonicalEventStream::append_after_expected_version` is the deterministic
domain operation for one command that emits zero, one, or multiple events.
It first compares the caller's `expected_version` with the loaded stream's
current version. A mismatch is rejected without changing the stream.

For a matching version, new events receive aggregate versions `v+1` through
`v+N` and command-local `event_index` values `0` through `N-1`. The operation
also requires one match identity and one causation command ID for the complete
batch, and rejects duplicate event IDs.

`C1LiveActionCommand::append_to_stream` is the single-event C1 adapter to this
same operation. It converts the accepted live action to an unsequenced event,
passes the command's expected version and causation ID to the generic stream,
and returns the resulting versioned event stream. It does not add a separate
versioning rule or persistence path.

The operation consumes the validated stream and returns a new stream. It does
not write PostgreSQL rows; transaction locking and persistence belong to the
following event-persistence increment.

## Verification

Database-free domain tests cover multi-event version/index assignment, stale
expected-version rejection, match and causation identity mismatches, and
preservation of the existing stream. No database state was changed.
