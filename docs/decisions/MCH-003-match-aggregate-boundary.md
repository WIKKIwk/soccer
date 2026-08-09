# MCH-003 — Match aggregate boundary

**Status:** Accepted  
**Decision date:** 2026-08-08  
**Depends on:** [MCH-001 match identity](MCH-001-match-identity.md), [MCH-002 initial match states](MCH-002-initial-match-states.md), [P0-005 canonical football vocabulary](P0-005-domain-vocabulary.md)

## Decision

`MatchAggregate` is the domain owner of one live match. Its first boundary
contains exactly:

- `MatchIdentity`, including the canonical `MatchId` and source `FixtureId`;
- `MatchLifecycle`, whose initial state is `MatchState::Draft`.

The aggregate exposes read-only identity and state accessors. The lifecycle
representation is not returned to callers, so future commands can enforce
state changes through aggregate methods rather than through an external
mutable state object.

The aggregate does not yet contain aggregate version, clock, confirmed
lineups, players-on-the-pitch state, canonical events, corrections, replay
logic, persistence, or transport types. Those concerns remain owned by their
scheduled MCH, CLK, EVT, and application increments.

The fixture remains scheduling-owned data. The match aggregate retains only the
typed identity link and does not copy or mutate fixture metadata.

## Verification

Database-free domain tests verify that a `MatchAggregate` preserves its
fixture-derived identity and always starts in `MatchState::Draft`. No
persistence or external state is touched.
