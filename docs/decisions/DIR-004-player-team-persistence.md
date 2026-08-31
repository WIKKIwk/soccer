# DIR-004 — Minimal player-to-team persistence

**Status:** Accepted  
**Decision date:** 2026-08-08  
**Depends on:** [DIR-002 minimal team persistence](DIR-002-team-persistence.md), [DIR-003 minimal person and player identity](DIR-003-person-player-identity.md), [ARC-007 forward-only migrations](ARC-007-forward-only-migrations.md)

## Decision

The first player registry persistence increment adds three separate tables:

- `persons` stores the canonical `PersonId` and required source-backed name;
- `players` stores the canonical `PlayerId` and its `PersonId` relationship;
- `team_player_assignments` stores the current `PlayerId` to `TeamId`
  relationship.

The tables retain the domain distinction between person, player profile, and
team. A player has at most one current team assignment in this thin vertical
slice. Reusing the same exact person, player, and assignment is idempotent;
changing the name, player ownership, or assigned team for an existing identity
returns a conflict. No row is silently moved or overwritten.

`team_player_assignments` is not a substitute for historical squad membership
or competition registration. Membership intervals, eligibility, and
competition-specific registration remain later increments.

## Explicit persistence boundary

One source-backed player can be persisted and read back through:

```text
ALLOW_DEVELOPMENT_DIRECTORY_SEED=1 \
  DATABASE_URL=<approved-target> \
  cargo run -p footballctl -- directory seed-player \
    --person-id <uuidv7> \
    --player-id <uuidv7> \
    --team-id <uuidv7> \
    --name <source-backed-name>
```

The command requires explicit UUIDv7 identities, a non-empty source-backed
person name, an existing team, and explicit directory-seed authorization. It
does not run migrations implicitly and does not invent player or team data.

## Verification status

Completed on 2026-08-14. The domain model, migration, repository transaction,
parser, and read-back path were exercised on the dedicated clean database
`football_socket_test_week5_official`. The focused `player_team_persistence`
fixture persisted and read back the source-backed `Odil Abjumajidov` assignment
to `Andijon`, with `Andijon` and `Buxoro` supplied as the two explicit teams.
It also verified guarded insertion, exact repeat idempotency, person/player/team
identity conflicts, missing-team rejection, and that rejected changes do not
move the original assignment. The source-backed reference is the PFL 2026
Superliga report: [PFL.UZ — Andijon Buxoro’ni ishonchli hisobda yengdi](https://pfl.uz/news/superliga-andijon-buxoroni-ishonchli-hisobda-yengdi).
