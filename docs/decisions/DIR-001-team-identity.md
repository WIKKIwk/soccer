# DIR-001 — Minimal team identity and name

**Status:** Accepted  
**Decision date:** 2026-08-08  
**Depends on:** [P0-005 canonical football vocabulary](P0-005-domain-vocabulary.md), [ARC-003 UUID strategy](ARC-003-uuid-strategy.md)

## Decision

The first directory increment defines a `Team` with:

- `TeamId`, a typed canonical UUID identity;
- `TeamName`, a non-empty name value object; and
- no implicit club, squad, competition, or fixture relationship.

`TeamName` rejects values that contain no non-whitespace content and otherwise
preserves the supplied text. It does not invent a localized translation or
silently normalize source data. Locale-aware names are a later directory
increment.

## Vocabulary boundary

A team is the sporting participant that can later be registered in a
competition and assigned as a fixture side. It is not a club organization, a
historical squad, or a match-specific lineup. Persistence is intentionally
deferred to DIR-002.

## Verification

Database-free domain tests verify typed-ID round trips, name validation, and
preservation of the supplied name. No team table or relationship migration is
added by DIR-001.
