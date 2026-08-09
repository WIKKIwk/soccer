# PRJ-002 — Match-summary projection schema

**Status:** Implemented; clean-database schema and constraint verification passed  
**Decision date:** 2026-08-09  
**Depends on:** [PRJ-001 synchronous score and timeline reduction](PRJ-001-synchronous-score-timeline-reduction.md), [ARC-009 synchronous and asynchronous projection boundaries](ARC-009-projection-boundaries.md), and [MCH-001 match identity from fixture](MCH-001-match-identity.md)

## Decision

`match_summary_projections` stores one disposable current summary row per live
match. `match_id` is the stable projection key and `fixture_id` is a required,
unique foreign key to the scheduling-owned fixture. A persisted match-aggregate
table does not exist yet, so the projection key intentionally has no fabricated
foreign key to a match table.

The schema stores only the summary fields whose owners are already defined:

- non-negative home and away goal totals bounded to the domain `u32` range;
- the current C1 lifecycle state using the established `MatchState` vocabulary;
- provisional or verified match quality state; and
- creation/update timestamps for operational row management.

The fixture remains the source for kickoff and home/away team identity. The
summary row therefore does not copy those values and create a second mutable
owner for scheduling data; a future query joins the fixture when it needs them.

The row defaults to a score of `0:0`, `draft`, and `provisional`. These are the
canonical initial values, not inferred live data. The projection remains
rebuildable from the canonical match stream and aggregate state.

## Deferred projection fields

- timeline rows belong to `PRJ-003`;
- lineup projection rows belong to `PRJ-004`;
- source aggregate version, projection version, freshness, rebuild, and error
  metadata belong to `PRJ-005` and later freshness work; and
- current clock persistence remains a match-engine critical-state increment.

No application repository or public endpoint is added by this schema-only
increment. Later writers must upsert this table inside the same transaction as
the source state they project and must never use it to overwrite canonical
events.

## PostgreSQL invariants

- one summary row per `match_id`;
- at most one summary row per `fixture_id`;
- every summary references an existing fixture;
- both scores are within `0..=4_294_967_295`;
- lifecycle state is one of the current C1 states; and
- verification state is `provisional` or `verified`.

## Verification

The ignored clean-database integration test
`match_summary_projection_schema_enforces_identity_defaults_and_state_bounds`
applies the complete migration set and verifies default values, fixture
identity uniqueness, fixture foreign-key rejection, score bounds, lifecycle
state bounds, and verification-state bounds. It passed against the dedicated
`football_socket_prj002` database. That disposable database is removed after
verification; the persistent project test database is not used or changed.
