# LIN-002 — Reject duplicate players in one lineup

**Status:** Accepted  
**Decision date:** 2026-08-08  
**Depends on:** [LIN-001 starter and substitute lineup entries](LIN-001-lineup-entries.md), [P0-005 canonical football vocabulary](P0-005-domain-vocabulary.md)

## Decision

One lineup collection may contain each `PlayerId` at most once. The invariant is
identity-based, so a player repeated once as a starter and once as a substitute
is still rejected. The validator does not compare names or raw UUID strings;
it uses the typed `PlayerId`.

This increment validates only collection-level uniqueness. Match ownership,
team ownership, eligibility, and persistence remain separate lineup and
competition concerns.

## Verification

Database-free domain tests accept distinct starter/substitute players and reject
the same player repeated across different roles with a typed validation error.
