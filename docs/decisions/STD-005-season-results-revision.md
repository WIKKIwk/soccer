# STD-005 — Season results revision

**Status:** Implemented; domain and repository contracts verified, clean-database acceptance pending
**Decision date:** 2026-08-10
**Depends on:** [STD-001 verified-result input contract](STD-001-verified-result-input-contract.md), [STD-002 played/won/drawn/lost reduction](STD-002-played-won-drawn-lost-reduction.md), [STD-003 goals and points reduction](STD-003-goals-and-points-reduction.md), and [ARC-009 projection boundaries](ARC-009-projection-boundaries.md)

## Decision

Each season owns a monotonic `results_revision` for the canonical verified
results that belong to that season. `SeasonResultsRevision` starts at zero and
advances by exactly one for each committed final or corrected verified result
accepted by the future application orchestration.

This is a source revision, not a standings projection generation. A standings
projection will later store the last `computed_results_revision`; equality of
those values is the freshness condition defined by ARC-009 and STD-006.

## Persistence and transaction boundary

The `seasons.results_revision` column is a non-negative PostgreSQL `BIGINT`
with a default of zero. Existing seasons receive revision zero when migration
`0022_season_results_revision.sql` is applied.

`advance_season_results_revision` performs the increment with one
`UPDATE ... RETURNING` statement inside the caller's transaction. The caller
must commit this transaction together with the verified-result state and its
outbox message. A rollback therefore does not publish a revision that has no
committed canonical result.

Identity-only season seeding remains idempotent and never resets an existing
revision. Loading a season preserves the stored revision.

## Ownership and deferred concerns

The match engine and verification workflow remain authoritative for deciding
which result is final and verified. This increment supplies the typed season
state and transaction-safe persistence seam; it does not yet connect the
revision advance to result verification, correction orchestration, standings
rows, `computed_results_revision`, or a public endpoint. Those boundaries
belong to STD-006 through STD-008 and the related application increments.

## Invariants

`SeasonResultsRevision` enforces the domain transition from the current value
to the next representable value and rejects overflow without mutating the
season. The database constraint rejects negative stored revisions. A season
created through `Season::new` starts at zero.

## Verification

The focused domain tests cover the zero starting value, one-step advancement,
and overflow preservation. Infrastructure unit tests pass, and the dedicated
clean-database integration test compiles and covers rollback, commit,
idempotent seeding, and persisted monotonic advancement. That integration test
remains ignored until an explicitly approved clean PostgreSQL target is
available for this increment.
