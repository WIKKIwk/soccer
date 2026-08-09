# PRJ-007 — Event/projection consistency tests

**Status:** Implemented; canonical replay, effective-stream, append, and persisted projection consistency verification passed  
**Decision date:** 2026-08-09  
**Depends on:** [PRJ-001 synchronous score and timeline reduction](PRJ-001-synchronous-score-timeline-reduction.md), [PRJ-003 match-timeline projection schema](PRJ-003-match-timeline-projection-schema.md), [PRJ-005 projection and aggregate versions](PRJ-005-projection-and-aggregate-versions.md), [PRJ-006 full match projection rebuild](PRJ-006-full-match-projection-rebuild.md), [COR-002 effective event-stream builder](COR-002-effective-event-stream-builder.md), and [ARC-009 projection boundaries](ARC-009-projection-boundaries.md)

## Decision

Projection consistency is verified by independently replaying the persisted
canonical event stream in the integration test and comparing that result with
the rows published by `rebuild_match_projection`. The test does not reuse the
rebuild function's private reduction result as its oracle; it loads the
canonical union, constructs a separate effective stream, and reduces it with
the domain `MatchProjection` API.

## Consistency invariants

For a current match projection:

- summary and timeline-head `source_aggregate_version` equal the canonical
  stream's current version, including source versions skipped by a void;
- summary and timeline-head `projection_version` are equal and current;
- summary home/away scores equal the independent effective-stream reduction;
- summary verification status follows the latest effective verification marker;
- timeline row count and order equal the reduced timeline;
- every timeline row preserves event ID, canonical aggregate version,
  command-local event index, C1 type, live period, and clock metadata; and
- every persisted timeline event exists in both the canonical and effective
  streams.

The test then appends a new valid canonical event, rebuilds again, and repeats
the comparison. This proves that source version, projection generation, and
timeline rows advance together rather than only matching one initial fixture.

The existing PRJ-006 integration test remains responsible for voided-goal
removal, generation replacement, failure-state publication, preservation of
the previous timeline on failure, and the explicit non-rebuild of lineups and
lifecycle state.

## Scope

This increment adds test coverage only. It does not add a production
consistency worker or allow a projection to repair canonical data. Each
ignored PostgreSQL test requires its own clean disposable database and is run
individually because the test preflight intentionally rejects an existing
`_sqlx_migrations` table. Lineup and lifecycle consistency remain deferred
until their canonical event sources exist.

## Verification

The following focused tests passed independently against the dedicated
`football_socket_prj007` database:

```text
match_projection_rebuilds_effective_score_timeline_and_versions: 1 passed
match_projection_rows_stay_consistent_with_canonical_replay_after_append: 1 passed
```

The consistency database applied all 21 migrations. After the append test,
the audit showed five canonical events, summary and timeline-head source
version 5 with projection version 2 and `is_current = true`, and two timeline
rows. After the rebuild/failure test, both roots exposed
`projection.rebuild_failed` while the prior timeline row remained. The
disposable database was removed and the persistent `football_socket_test`
database remained unchanged.
