# DIR-003 — Minimal person and player identity

**Status:** Accepted  
**Decision date:** 2026-08-08  
**Depends on:** [P0-005 canonical football vocabulary](P0-005-domain-vocabulary.md), [ARC-003 UUID strategy](ARC-003-uuid-strategy.md), [DIR-001 minimal team identity and name](DIR-001-team-identity.md)

## Decision

The first person/player increment defines two distinct directory concepts:

- `PersonId` and `Person`, representing a real-world human identity;
- `PlayerId` and `Player`, representing a football participation profile owned by
  one `PersonId`.

`Person` owns one required source-backed `PersonName`. The value rejects input
that contains no non-whitespace content and otherwise preserves the supplied
text. No translation, alias, transliteration, or silent normalization is
invented.

`Player` owns only its typed profile ID and the typed `PersonId` relationship.
Team assignment, squad membership, competition registration, match eligibility,
position, shirt number, and biographical attributes are later concerns. In
particular, a player profile does not replace the person identity and a person
does not implicitly become assigned to a team.

Both identifiers are UUID-backed domain newtypes. They are injected from an
application or infrastructure boundary; the domain does not generate IDs.

## Verification

Database-free domain tests verify typed-ID round trips, source-name preservation,
blank-name rejection, and explicit player-to-person ownership. Persistence and
team assignment are intentionally deferred to `DIR-004`.
