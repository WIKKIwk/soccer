# LIN-003 — Draft lineup persistence

**Status:** Accepted  
**Decision date:** 2026-08-08  
**Depends on:** [SCH-001 minimal fixture](SCH-001-minimal-fixture.md), [LIN-001 starter and substitute lineup entries](LIN-001-lineup-entries.md), [LIN-002 duplicate-player validation](LIN-002-unique-lineup-players.md), [DIR-004 minimal player-to-team persistence](DIR-004-player-team-persistence.md), [ARC-007 forward-only migrations](ARC-007-forward-only-migrations.md)

## Decision

The first draft-lineup persistence increment adds:

- a minimal `fixtures` storage table containing the SCH-001 fixture fields;
- a `lineups` table with one draft lineup per fixture/team side; and
- `lineup_entries` with typed player identity and starter/substitute role.

The repository persists a validated `Fixture` and `DraftLineup` in one
transaction. It rejects a lineup whose team is not the fixture’s home or away
side, rejects players not currently assigned to that team, preserves duplicate
player protection, and reads the same draft lineup back. Repeating the exact
same input is idempotent; changing an existing fixture or lineup identity is a
conflict.

The fixture table retains the canonical competition and season IDs without
creating guessed master-data rows. Competition/season foreign keys are added
by the later CMP persistence migrations after those owning tables exist; this
is an explicit expand step, not a second identity source.

Lineup confirmation, status transitions, match ownership, eligibility beyond
current team assignment, and CLI commands remain later concerns.

## Verification status

The domain model, migration, transactional repository, role decoding, and
database-free tests are implemented. LIN-003 remains incomplete until the
ordered migrations and one approved source-backed draft lineup are applied and
read back on an approved clean database target.
