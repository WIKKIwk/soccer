# STD-003 — Goals and points reduction

**Status:** Implemented; goals/points reduction and explicit policy boundary verified
**Decision date:** 2026-08-09
**Depends on:** [STD-001 verified-result input contract](STD-001-verified-result-input-contract.md), [STD-002 played/won/drawn/lost reduction](STD-002-played-won-drawn-lost-reduction.md), [P0-005 canonical football vocabulary](P0-005-domain-vocabulary.md), and [ARC-009 projection boundaries](ARC-009-projection-boundaries.md)

## Decision

`football-domain::standings` reduces one `VerifiedMatchResult` into one
`MatchGoalsPointsReduction`. Each team receives:

- `goals_for` from the score credited to that team;
- `goals_against` from the opponent's score; and
- `points` selected from an explicitly supplied `PointsRule`.

The home/away mapping is deterministic:

```text
home: goals_for = home_goals, goals_against = away_goals
away: goals_for = away_goals, goals_against = home_goals
```

The reducer maps `MatchOutcome` to the supplied policy as follows:

```text
home win: (home win_points, away loss_points)
draw:     (home draw_points, away draw_points)
away win: (home loss_points, away win_points)
```

The first C1 example uses the conventional `3/1/0` values by explicitly
passing `PointsRule::new(3, 1, 0)`. The reducer does not silently choose those
values. The points rule is not persisted, versioned, or treated as the final
competition-rules model in this increment; RUL-001 through RUL-004 own that
future boundary.

## Invariants and ownership

The input remains `VerifiedMatchResult`, so provisional operator observations
and disposable summary projections cannot enter this reducer through the
typed standings path. A verified 0–0 result produces zero goals for and
against and the supplied draw points for both teams.

The reduction preserves `season_id`, `match_id`, `fixture_id`, and canonical
`source_aggregate_version` with the two deltas. It does not aggregate multiple
matches, calculate goal difference or positions, apply point adjustments, or
replace a previous corrected result. Those concerns belong to later standings
or rules increments.

## Implementation

`PointsRule` carries explicit win/draw/loss values. `TeamGoalsPointsDelta`
holds one team's goals and points contribution, and
`MatchGoalsPointsReduction` holds both sides plus source provenance.
`reduce_goals_and_points` and the corresponding `VerifiedMatchResult` method
are pure, database-independent domain operations.

No migration, standings writer, public endpoint, or realtime message was
added.

## Verification

Focused domain tests cover home-win goals and `3/1/0` points, draw goals and
points, away-win goal inversion, and a non-standard explicit policy to prove
that the reducer does not hard-code one competition rule. The command
`cargo test -p football-domain standings --lib` passed with 11 tests and no
failures.

The full domain suite passed with 141 tests. Workspace formatting, all-targets
check, clippy with `-D warnings`, and the DB-disabled full workspace test
command also passed.
