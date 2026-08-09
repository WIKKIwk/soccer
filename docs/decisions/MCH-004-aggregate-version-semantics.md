# MCH-004 — Aggregate version semantics

**Status:** Accepted  
**Decision date:** 2026-08-08  
**Depends on:** [MCH-003 match aggregate boundary](MCH-003-match-aggregate-boundary.md), [ARC-008 command, event, and version semantics](ARC-008-command-event-version-semantics.md)

## Decision

The match aggregate carries a typed `AggregateVersion` initialized to `0`.
The version is a monotonic sequence for canonical match events; it is not a
UUID, timestamp, database row version, or transport response version.

For current version `v` and an accepted command that emits `N` canonical
events, the event versions are assigned contiguously as:

```text
v + 1, v + 2, ..., v + N
```

An explicitly accepted zero-event operation leaves the version unchanged. A
version advance that would overflow `u64` is rejected as a domain error.
Commands that mutate the aggregate must provide `expected_version`; stale
values are conflicts and never overwrite a newer stream. The aggregate
version is the ordering authority for match events; UUID and wall-clock order
are not substitutes.

This increment adds the typed version value and the aggregate's initial
version accessor. It does not yet implement command admission, event
envelopes, event persistence, or version mutation during command execution.

## Verification

Database-free domain tests verify initial version `0`, event-count advancement,
zero-event stability, overflow rejection, and that every new
`MatchAggregate` starts at `AggregateVersion::initial()`.
