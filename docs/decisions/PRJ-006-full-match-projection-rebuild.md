# PRJ-006 — Full match projection rebuild

**Status:** Implemented; effective-stream rebuild, publication, idempotency, and failure-state verification passed  
**Decision date:** 2026-08-09  
**Depends on:** [PRJ-001 synchronous score and timeline reduction](PRJ-001-synchronous-score-timeline-reduction.md), [PRJ-002 match-summary projection schema](PRJ-002-match-summary-projection-schema.md), [PRJ-003 match-timeline projection schema](PRJ-003-match-timeline-projection-schema.md), [PRJ-004 match-lineup projection schema](PRJ-004-match-lineup-projection-schema.md), [PRJ-005 projection and aggregate versions](PRJ-005-projection-and-aggregate-versions.md), [ARC-002 partial event sourcing](ARC-002-partial-event-sourcing.md), and [ARC-009 projection boundaries](ARC-009-projection-boundaries.md)

## Decision

`rebuild_match_projection` is the infrastructure boundary for rebuilding the
currently implemented match score and timeline read model. It requires an
explicit `MatchId`, `FixtureId`, and UTC computation timestamp. It does not
infer fixture identity from an operator assignment or fabricate a match row.

The fixture row is locked with `FOR UPDATE` and supplies the home and away
team IDs used by `MatchProjection`. An existing summary projection must point
to the same explicit fixture. Missing summary or timeline-head roots are
created with the existing counterpart's freshness metadata when one root
already exists; both roots are then moved into the committed rebuilding state.

## Publication protocol

The rebuild has two transaction boundaries:

1. The prepare transaction locks the fixture and projection roots, validates
   their identity and projection generation, creates missing roots, and
   commits `is_rebuilding = TRUE`, `is_current = FALSE`, and a cleared error.
2. The publication transaction loads the canonical match event stream,
   builds its effective correction-resolved view, reduces it through
   `MatchProjection`, replaces all timeline rows, and publishes the summary
   and timeline head with one new projection generation. The roots receive the
   canonical source version, UTC computation timestamp, and current metadata
   only after the complete timeline replacement succeeds.

The effective stream keeps canonical aggregate versions and therefore may
contain gaps after a void or replacement. The published source version is the
latest canonical version, while timeline rows contain only effective C1
events. Verification status is derived from the latest effective verification
marker; absent or reopened verification is provisional, and the latest
`MatchVerified` marker is verified.

Projection generation is shared by the summary and timeline head and advances
monotonically from the previously published generation. A mismatch between
the two roots is rejected instead of silently choosing one version.

## Failure and scope boundaries

If canonical loading, effective-stream validation, reduction, or publication
fails, the publication transaction rolls back. Existing timeline rows and the
previous summary values remain intact. A separate failure transaction marks
both roots `is_rebuilding = FALSE`, `is_current = FALSE`, and
`last_error_code = 'projection.rebuild_failed'`. If that state cannot be
persisted, the returned error retains both the rebuild failure and the state
persistence failure.

This increment intentionally does not derive `match_state`: the current
canonical event union has no lifecycle or clock events, so guessing a lifecycle
state would fabricate business truth. An existing summary `match_state` is
preserved. It also does not rebuild lineup projections: draft lineups remain
their scheduling source and canonical lineup events do not yet exist. The
lineup read model is left untouched until its canonical reduction contract is
implemented.

## Verification

The ignored clean-database integration test
`match_projection_rebuilds_effective_score_timeline_and_versions` applies all
21 migrations against the dedicated `football_socket_prj006` database and
verifies:

- a voided goal is absent from the effective score and timeline;
- a live yellow card remains in canonical order with its source version;
- a verified marker produces match-level `verified` status;
- summary and timeline-head generations publish together;
- a second rebuild advances the generation and leaves one timeline row;
- existing `match_state` and lineup projection freshness are preserved; and
- an invalid later canonical payload leaves the prior rows and values intact
  while both roots expose the rebuild failure state.

The test passed, the database was audited, and the disposable database was
removed. The persistent `football_socket_test` database remained unchanged.
