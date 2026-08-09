# STD-001 — Verified-result input contract

**Status:** Implemented; domain contract and rejection invariants verified
**Decision date:** 2026-08-09
**Depends on:** [MCH-001 match identity](MCH-001-match-identity.md), [SCH-001 minimal fixture](SCH-001-minimal-fixture.md), [QUA-001 provisional and verified state](QUA-001-provisional-verified-state.md), [QUA-003 match verification event](QUA-003-match-verification.md), and [PRJ-006 full match projection rebuild](PRJ-006-full-match-projection-rebuild.md)

## Decision

The standings reducer accepts a `football_domain::VerifiedMatchResult` as its
single-match input. The value is created from the existing `MatchIdentity` and
`Fixture` plus the final score, match-level verification status, and canonical
source aggregate version.

The constructor is an admission boundary, not a second score authority. The
caller must supply home and away goals obtained by replaying the canonical
match event stream after correction/void resolution. A provisional operator
observation or a disposable match-summary projection is not a valid source for
this value.

The contract contains exactly the data needed to reduce one result into
season standings:

- `season_id`, taken from the fixture;
- `match_id`, taken from the match identity;
- `fixture_id`, taken from the fixture;
- home and away team IDs, taken from the fixture;
- non-negative home and away goals; and
- the canonical `source_aggregate_version`.

The value has no mutable verification-status field. Successful construction
proves `MatchVerificationStatus::Verified`; provisional state is rejected.
The source aggregate version must be positive because the verification marker
itself is a canonical event and therefore cannot exist at version zero.

## Invariants

`VerifiedMatchResult::try_new` rejects:

1. `provisional` status, before any standings reduction can occur;
2. a match identity whose fixture ID differs from the supplied fixture; and
3. source aggregate version zero, which denotes an unpublished/empty stream.

The fixture already owns the home/away distinctness invariant. A verified
0–0 result is valid and is preserved without inventing an outcome. Goals,
played/won/drawn/lost, points, tiebreak position, and season
`results_revision` remain separate concerns for STD-002 through STD-008.

## Authority and ownership

The match event stream remains authoritative. `football-domain::standings`
owns the typed input consumed by the future standings reducer; match engine
owns canonical event replay and verification state; application orchestration
will later connect the verified replay to this input; infrastructure owns
persistence and revision delivery. No summary projection, operator identity,
or transport JSON is treated as a standings authority.

This increment adds no migration, result reducer, standings table, revision
counter, or public endpoint.

## Verification

The focused domain tests cover:

- preservation of all contract fields;
- rejection of provisional state;
- rejection of source version zero;
- rejection of a mismatched match/fixture identity; and
- acceptance of a verified 0–0 result.

The focused command `cargo test -p football-domain standings --lib` passed with
5 tests and no failures.
