# STD-004 — Configurable tiebreak comparison

**Status:** Implemented; ordered metric comparison and policy invariants verified
**Decision date:** 2026-08-09
**Depends on:** [STD-002 played/won/drawn/lost reduction](STD-002-played-won-drawn-lost-reduction.md), [STD-003 goals and points reduction](STD-003-goals-and-points-reduction.md), [P0-005 canonical football vocabulary](P0-005-domain-vocabulary.md), and [ARC-009 projection boundaries](ARC-009-projection-boundaries.md)

## Decision

`football-domain::standings` compares two `StandingComparisonRow` values with
an explicit `TiebreakPolicy`. A policy is a non-empty, ordered, duplicate-free
list of `TiebreakCriterion` values. Each criterion names both:

- a metric: `Points`, `GoalDifference`, `GoalsFor`, `GoalsAgainst`, `Won`,
  `Drawn`, `Lost`, or `Played`; and
- a direction: `Ascending` or `Descending`.

`compare_standing_rows` evaluates the policy from first to last and returns
the first non-equal metric. `Ordering::Less` means the left row ranks before
the right row and can therefore be passed directly to `sort_by`.

If every configured criterion is equal, the comparator returns
`Ordering::Equal`. Team UUID, insertion order, or an undocumented SQL order
is never used as a hidden football tiebreaker. The caller may therefore
represent a genuine tied position explicitly.

## Ownership and boundary

This increment provides a pure comparison seam for the standings engine. It
does not persist the policy, assign a position, implement head-to-head or
fair-play rules, or decide the official order for O‘zbekiston Superligasi
2026. The competition/rules increments `RUL-001` through `RUL-004` remain the
owners of the versioned, published policy that will later be supplied here.

The comparison row is an input snapshot containing the current aggregate
played/won/drawn/lost, goals-for/goals-against, and points values. It is not a
second canonical score authority and carries no projection freshness claim.

## Invariants

`TiebreakPolicy::try_new` rejects an empty policy and repeated metrics,
including the same metric configured with different directions. Criterion
order is preserved exactly. Ascending metrics rank lower values first;
descending metrics rank higher values first. Goal difference is computed as a
signed value from goals-for minus goals-against.

## Implementation

The policy, criteria, comparison row, typed policy errors, and comparator live
in `football-domain::standings`. No migration, standings writer, public
endpoint, or realtime message was added.

## Verification

Focused domain tests cover first-difference comparison, policy-order changes,
ascending direction, goal-difference comparison, genuine equal rows without
identity fallback, and empty/duplicate policy rejection. The command
`cargo test -p football-domain standings --lib` passed with 16 tests and no
failures.

The full domain suite passed with 146 tests. Workspace formatting, all-targets
check, clippy with `-D warnings`, and the DB-disabled full workspace test
command also passed.
