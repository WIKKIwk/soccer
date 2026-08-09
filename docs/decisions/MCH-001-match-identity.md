# MCH-001 — Match identity from fixture

**Status:** Accepted  
**Decision date:** 2026-08-08  
**Depends on:** [SCH-001 minimal fixture](SCH-001-minimal-fixture.md), [ARC-002 partial event sourcing](ARC-002-partial-event-sourcing.md), [ARC-003 UUID strategy](ARC-003-uuid-strategy.md), [P0-005 canonical football vocabulary](P0-005-domain-vocabulary.md)

## Decision

The match engine introduces a typed `MatchId` and a `MatchIdentity` containing
the source `FixtureId`. `MatchIdentity::from_fixture` is the only constructor
for this increment, so a match identity cannot be created without an explicit
fixture link.

This increment does not add lifecycle state, aggregate version, clock, lineup
confirmation, canonical events, corrections, or replay. Those concerns remain
separate MCH and CLK increments. A fixture remains scheduling data; the match
identity is the handle for the future live match aggregate.

## Verification

Database-free domain tests verify creation from one fixture and UUID round-trip
behavior. No database row or event is created by this identity type.
