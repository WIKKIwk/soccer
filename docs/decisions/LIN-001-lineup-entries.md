# LIN-001 — Starter and substitute lineup entries

**Status:** Accepted  
**Decision date:** 2026-08-08  
**Depends on:** [P0-005 canonical football vocabulary](P0-005-domain-vocabulary.md), [DIR-003 minimal person and player identity](DIR-003-person-player-identity.md)

## Decision

The first lineup increment defines a `LineupEntry` containing:

- one typed `PlayerId`; and
- one explicit `LineupRole`, either `Starter` or `Substitute`.

This is a match-specific nomination vocabulary. It does not copy person names,
team names, or long-term squad state into the entry. A player may be represented
by at most one entry in a lineup; the collection-level duplicate invariant is
implemented and tested separately by `LIN-002`.

## Verification

Database-free domain tests verify both role constructors, typed player identity,
stable role display values, and the distinction between starter and substitute.
