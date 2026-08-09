# PRJ-004 — Match-lineup projection schema

**Status:** Implemented; clean-database match, side, status, and player-role verification passed  
**Decision date:** 2026-08-09  
**Depends on:** [LIN-001 starter and substitute lineup entries](LIN-001-lineup-entries.md), [LIN-002 unique lineup players](LIN-002-unique-lineup-players.md), [LIN-003 draft lineup persistence](LIN-003-draft-lineup-persistence.md), [MCH-006 pre-match and lineup-confirmed transitions](MCH-006-pre-match-lineup-transitions.md), and [ARC-009 projection boundaries](ARC-009-projection-boundaries.md)

## Decision

The lineup read model is split into three disposable projection tables:

1. `match_lineup_projections` has one root row per match and one unique source
   `fixture_id`;
2. `match_lineup_team_projections` has at most one `home` and one `away` row
   per match, each with `provisional` or `confirmed` status; and
3. `match_lineup_entry_projections` stores the typed player ID and
   `starter`/`substitute` role for each side.

The root keeps the fixture link, while the fixture remains the source of home
and away team identity. Team names, player names, and squad metadata are not
copied into the projection. Public queries join the root to `fixtures` and
resolve the side to the fixture’s home or away team.

The existing `lineups` and `lineup_entries` tables remain the draft lineup
write model owned by the lineup persistence increment. The projection does not
foreign-key to those draft rows: match-scoped provisional/confirmed state is a
read model of the match context and must remain rebuildable from canonical
match input rather than making a draft row authoritative for public state.

## PostgreSQL invariants

- one projection root exists per `match_id`;
- one match projection exists per `fixture_id`;
- each side is exactly `home` or `away` and can have at most one status row;
- lineup status is `provisional` or `confirmed`;
- every entry references an existing player and an existing match-side row;
- entry role is `starter` or `substitute`;
- one player appears at most once in one side lineup; and
- one player appears at most once across both sides of one match.

Team-side membership is represented by the fixture-side value instead of a
second mutable `team_id` owner. The application projection writer must still
validate the source fixture and player eligibility before inserting derived
rows; the schema does not fabricate historical squad membership.

Child rows use `ON DELETE CASCADE` from the projection root. Dropping one
disposable match projection therefore removes its side and entry rows without
touching fixtures, players, draft lineups, or canonical match events.

## Deferred fields and behavior

- source aggregate/projection freshness versions belong to `PRJ-005`;
- canonical lineup event reduction and full rebuild behavior belong to later
  projection/application increments; and
- public REST mapping remains a later endpoint increment.

This migration creates storage shape only. It does not mark a lineup confirmed,
change match lifecycle state, or add a write path.

## Verification

The ignored clean-database integration test
`match_lineup_projection_schema_enforces_match_side_status_and_player_roles`
applies the complete migration set and verifies valid home/away rows, status
and role round-trip, fixture uniqueness, missing-parent/player rejection,
invalid side/status/role rejection, duplicate-player protection, and root
cascade cleanup. It passed against the dedicated `football_socket_prj004`
database. That disposable database is removed after verification; the
persistent project test database is not used or changed.
