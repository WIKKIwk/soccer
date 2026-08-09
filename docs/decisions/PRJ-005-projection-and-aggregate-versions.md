# PRJ-005 — Projection and aggregate versions

**Status:** Implemented; clean-database freshness, publication, rebuild, error, and constraint verification passed  
**Decision date:** 2026-08-09  
**Depends on:** [PRJ-002 match-summary projection schema](PRJ-002-match-summary-projection-schema.md), [PRJ-003 match-timeline projection schema](PRJ-003-match-timeline-projection-schema.md), [PRJ-004 match-lineup projection schema](PRJ-004-match-lineup-projection-schema.md), [ARC-004 UTC timezone rules](ARC-004-utc-timezone-rules.md), and [ARC-009 projection boundaries](ARC-009-projection-boundaries.md)

## Decision

Each match read model stores the source aggregate version and the published
projection generation together with enough freshness metadata to disclose an
uncomputed, rebuilding, failed, or current state.

The summary and lineup projection roots receive these fields directly:

- `source_aggregate_version BIGINT NOT NULL DEFAULT 0`;
- `projection_version BIGINT NOT NULL DEFAULT 0`;
- `computed_at_utc TIMESTAMPTZ`, nullable until a generation is published;
- `is_rebuilding BOOLEAN NOT NULL DEFAULT FALSE`;
- `is_current BOOLEAN NOT NULL DEFAULT FALSE`; and
- optional machine-readable `last_error_code`.

Timeline rows are event-level and may be empty for a match, so their
match-level freshness metadata is stored once in
`match_timeline_projection_heads`. The head uses the same fields and
invariants without copying canonical event payload or creating a new event
authority.

The initial metadata state is deliberately not current:

```text
source_aggregate_version = 0
projection_version       = 0
computed_at_utc          = NULL
is_rebuilding            = FALSE
is_current               = FALSE
last_error_code          = NULL
```

`source_aggregate_version = 0` is a valid aggregate version, so the explicit
unpublished generation (`projection_version = 0` and `computed_at_utc IS
NULL`) plus `is_current = FALSE` is what identifies an uncomputed row. A
future writer may publish an aggregate at source version zero, but it must
assign a positive projection generation and a compute timestamp before
marking the row current.

## PostgreSQL invariants

- source and projection versions cannot be negative;
- a current row must have a positive published projection generation;
- a current row must have `computed_at_utc` and no error code;
- a row cannot be rebuilding and current at the same time;
- an error code must be non-blank when present; and
- the timeline head has at most one metadata row per match.

The migration stores `TIMESTAMPTZ` for the compute instant, following the
UTC persistence rule. It does not add a writer, rebuild algorithm, canonical
match table, or public endpoint; those remain owned by later increments.

## Verification

The ignored clean-database integration test
`projection_versions_store_shared_freshness_metadata_for_all_match_read_models`
applies all 21 migrations and verifies:

- identical metadata defaults on summary, lineup, and timeline-head rows;
- independent source and projection version round-trips;
- current publication with a compute timestamp;
- representable rebuilding and error states;
- rejection of negative versions, unpublished current generations, missing
  compute timestamps, current/rebuilding overlap, current/error overlap, and
  blank error codes; and
- one-row timeline-head identity.

It passed against the dedicated `football_socket_prj005` database. The
database was audited and removed after the test. The persistent
`football_socket_test` database remained unchanged.
