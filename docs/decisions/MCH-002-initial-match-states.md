# MCH-002 — Initial match states

**Status:** Accepted  
**Decision date:** 2026-08-08  
**Depends on:** [MCH-001 match identity](MCH-001-match-identity.md), [P0-005 canonical football vocabulary](P0-005-domain-vocabulary.md), [ARC-002 partial event sourcing](ARC-002-partial-event-sourcing.md)

## Decision

The first C1 match lifecycle uses the following state vocabulary:

```text
DRAFT
  → SCHEDULED
  → PRE_MATCH
  → LINEUPS_PROVISIONAL
  → LINEUPS_CONFIRMED
  → FIRST_HALF
  → HALF_TIME
  → SECOND_HALF
  → FINISHED
```

`MatchState::Draft` is the single canonical initial state. Every newly created
`MatchLifecycle` starts there through `MatchState::initial()`; a match cannot
start directly in a live, completed, postponed, or cancelled state.

The first operational extensions are part of the same state vocabulary:

- `Interrupted` is only entered from a live period and resumes to the exact
  live period that was interrupted.
- `Postponed` and `Cancelled` are pre-match outcomes reachable from
  `Scheduled` or `PreMatch`.

Extra time, penalty shootouts, delayed, and abandoned states are outside this
initial C1 lifecycle. They require separate decisions and transition tests.

This increment defines the state vocabulary and initial-state invariant. It
does not introduce aggregate versioning, clock values, canonical events,
commands, or persistence.

## Verification

Database-free domain tests verify that `MatchState::initial()` is `Draft` and
that `MatchLifecycle::new()` uses that canonical initial state. Existing
transition tests continue to cover the initial flow and operational extensions.
