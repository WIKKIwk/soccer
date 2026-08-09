# STD-002 — Played/won/drawn/lost reduction

**Status:** Implemented; verified-result outcome reduction and invariants passed
**Decision date:** 2026-08-09
**Depends on:** [STD-001 verified-result input contract](STD-001-verified-result-input-contract.md), [MCH-001 match identity](MCH-001-match-identity.md), and [QUA-001 provisional and verified state](QUA-001-provisional-verified-state.md)

## Decision

`football-domain::standings` reduces one `VerifiedMatchResult` into one
`MatchOutcomeReduction`. The reduction contains one `TeamOutcomeDelta` for the
home team and one for the away team, plus the season, match, fixture, and
canonical source version needed to trace the contribution.

The outcome is derived only from the verified final score:

| Score relation | Home delta | Away delta |
|---|---|---|
| home goals > away goals | played 1, won 1 | played 1, lost 1 |
| home goals = away goals | played 1, drawn 1 | played 1, drawn 1 |
| home goals < away goals | played 1, lost 1 | played 1, won 1 |

For every team delta, `won + drawn + lost = played = 1`. No outcome counter
is inferred from operator status, lifecycle state, missing data, or a match
summary projection. The `VerifiedMatchResult` input already rejects
provisional state.

## Ownership and deferred concerns

The match engine remains authoritative for canonical event replay and final
verification. The standings module owns this deterministic one-match outcome
reduction. It does not yet aggregate multiple results, deduplicate corrected
results, persist standings rows, advance `results_revision`, reduce goals,
calculate points, or compare tiebreakers. Those concerns remain with the
following STD increments.

A verified 0–0 result is a draw for both teams. A later corrected/reverified
result is a new source version; replacing an earlier contribution belongs to
the revision/rebuild workflow rather than this pure reducer.

## Implementation

`MatchOutcome` classifies the verified score as `HomeWin`, `Draw`, or
`AwayWin`. `reduce_played_won_drawn_lost` produces the typed home/away deltas;
`VerifiedMatchResult::reduce_played_won_drawn_lost` exposes the same operation
through the input contract.

No migration, database writer, public endpoint, or realtime message was added.

## Verification

Focused domain tests cover home win, away win, draw, source identity/version
preservation, and the invariant that each side receives exactly one played
and one outcome counter. `cargo test -p football-domain standings --lib`
passed with 8 tests and no failures.

The full domain suite passed with 138 tests. Workspace formatting, all-targets
check, clippy with `-D warnings`, and the DB-disabled full workspace test
command also passed.
