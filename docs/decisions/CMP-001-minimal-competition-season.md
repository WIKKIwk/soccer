# CMP-001 — Minimal competition and season

**Status:** Accepted  
**Decision date:** 2026-08-08  
**Depends on:** [P0-001 product contract](P0-001-product-contract.md), [P0-005 canonical football vocabulary](P0-005-domain-vocabulary.md), [ARC-003 UUID strategy](ARC-003-uuid-strategy.md)

## Decision

The first competition increment defines:

- `CompetitionId`, `Competition`, and a required source-backed
  `CompetitionName`;
- `SeasonId`, `Season`, a typed `CompetitionId` owner, and a positive
  `SeasonYear`.

The current product contract is represented as competition name
`O‘zbekiston Superligasi` and season year `2026`. Names preserve supplied
source text after rejecting values containing no non-whitespace content. The
domain does not invent translations, statuses, competition types, date
intervals, stages, rounds, groups, or versioned rules.

The season carries its owning competition explicitly. A year is a minimal
label for this first slice; it is not treated as a start/end date or as a
substitute for later season lifecycle and rules data.

## Verification

Database-free domain tests verify typed UUID round trips, source-name
preservation, blank-name rejection, explicit competition ownership, and
positive season-year validation. Persistence is deferred to later competition
and season repository increments.
