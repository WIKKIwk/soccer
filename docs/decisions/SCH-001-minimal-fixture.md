# SCH-001 — Minimal fixture with UTC kickoff

**Status:** Accepted  
**Decision date:** 2026-08-08  
**Depends on:** [CMP-001 minimal competition and season](CMP-001-minimal-competition-season.md), [DIR-001 minimal team identity and name](DIR-001-team-identity.md), [ARC-003 UUID strategy](ARC-003-uuid-strategy.md), [ARC-004 UTC and timezone rules](ARC-004-utc-timezone-rules.md)

## Decision

The first scheduling increment defines a `Fixture` with:

- a typed `FixtureId`;
- explicit competition and season ownership inherited from one `Season`;
- distinct typed home and away `TeamId` values; and
- a canonical `UtcInstant` kickoff.

`UtcInstant` accepts an offset-aware `OffsetDateTime` at the boundary and
normalizes it to UTC. A local date-time without an offset is not an input to the
value object, and host-machine timezone is never consulted. The fixture rejects
the same team being used for both home and away.

Venue, stage, round, scheduling status, postponement/cancellation metadata,
officials, persistence, and source-offset evidence are outside SCH-001. The
application/evidence boundary may retain the original source offset separately
when required by ARC-004.

## Verification

Database-free domain tests verify fixture identity and season ownership,
home/away distinction, UUID round-trip behavior, and explicit-offset to UTC
normalization without using the host timezone.
