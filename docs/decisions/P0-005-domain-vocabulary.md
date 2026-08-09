# P0-005 — Canonical football vocabulary

**Status:** Accepted for C1 implementation  
**Decision date:** 2026-08-08  
**Depends on:** [P0-001 product contract](P0-001-product-contract.md)

These definitions are the canonical domain vocabulary for the first C1 implementation. Code, database names, API contracts, tests, and operator commands must preserve these distinctions.

## Definitions

### Club

A football organization identity. A club can be related to one or more teams over time, but it is not itself the competition participant in a match.

### Team

A sporting participant that can be registered in a competition and assigned as the home or away side of a fixture. A team may be related to a club, but the team identity remains distinct from the club identity.

### Squad

The historical set of players associated with a team during membership intervals. Squad membership is not a match nomination and does not by itself replace competition registration or eligibility rules.

### Lineup

A match-specific nomination of players, divided into starters and substitutes. A lineup starts as provisional and can become confirmed. A player may appear at most once in one lineup. Lineup state belongs to the match context, not to the team’s long-term squad record.

### Fixture

A scheduled competition contest contract. It identifies the competition context, stage/round, home team, away team, kickoff, venue, and scheduling status. A fixture exists before live play and is the source from which a match aggregate is created.

### Match

The live football domain aggregate created from a fixture. It owns the match lifecycle, confirmed lineup state, clock, players-on-the-pitch state, aggregate version, canonical match events, corrections, voids, and deterministic replay behavior.

### Stage

An ordered phase of a competition season with a defined format and versioned rules context. A stage may contain groups, rounds, ties, and fixtures. Stage structure must not be inferred from a match’s live state.

### Round

An ordered scheduling unit inside a stage. A round groups fixtures that belong to the same competition scheduling unit; it is not a match, a stage, or a tie.

### Tie

An advancement unit that groups one or more fixtures between the same competition participants, normally for a knockout context. A tie is optional for ordinary league fixtures and does not own live events; its fixtures create and reference their own match aggregates.

## Relationship model

```text
Competition
  → Season
    → Stage
      → Group / Round / Tie
        → Fixture
          → Match aggregate

Club
  → Team
    → historical Squad membership
      → eligible player
        → match Lineup
          → Match participation and canonical events
```

The tie-to-fixture relationship is optional. A fixture has exactly one home team and one away team once scheduled. A match has exactly one source fixture.

## Non-negotiable distinctions

| Must remain distinct | Reason |
|---|---|
| Club vs team | Organizational identity and sporting participant have different lifecycles and history. |
| Squad vs lineup | Long-term membership cannot be overwritten by one match’s nomination. |
| Fixture vs match | Scheduling data and live aggregate have different ownership and state transitions. |
| Stage vs round | Competition phase and scheduling unit have different rule and ordering responsibilities. |
| Tie vs fixture | A knockout advancement unit may contain multiple fixtures. |
| Player profile vs person | A person identity and football participation profile are separate directory concepts. |
| Competition registration vs squad membership | Association with a team does not automatically establish competition eligibility. |

## Ownership boundaries

- Directory owns clubs, teams, persons, player profiles, localized names, and historical squad membership.
- Competition owns competitions, seasons, stages, groups, rounds, ties, and versioned rules.
- Scheduling owns fixtures and scheduling metadata.
- Match Engine owns the match aggregate and live match behavior.
- Standings and Statistics consume verified or canonical match results; they do not own the canonical score.

## Acceptance criteria

P0-005 is complete when the definitions and ownership boundaries above can be used without treating any pair of distinct concepts as interchangeable in the first schema and domain tests.

