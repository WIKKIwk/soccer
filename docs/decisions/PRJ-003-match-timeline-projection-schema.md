# PRJ-003 — Match-timeline projection schema

**Status:** Implemented; clean-database source, ordering, and C1-boundary verification passed  
**Decision date:** 2026-08-09  
**Depends on:** [PRJ-001 synchronous score and timeline reduction](PRJ-001-synchronous-score-timeline-reduction.md), [PRJ-002 match-summary projection schema](PRJ-002-match-summary-projection-schema.md), [EVT-005 persisted event metadata](EVT-005-persist-event-metadata.md), and [ARC-009 synchronous and asynchronous projection boundaries](ARC-009-projection-boundaries.md)

## Decision

`match_timeline_projections` stores one ordered, metadata-only row for each
canonical C1 event included in the effective match timeline. The canonical
payload and correction history remain in `match_events`; the projection does
not copy payload JSON or become a second source of truth.

Each row keeps the fields already defined by `TimelineEntry`:

- canonical `event_id` and `match_id`;
- aggregate version and command-local event index;
- the C1 event type;
- the live match period; and
- the non-negative football clock in milliseconds.

`event_id` is the projection primary key. The migration adds a composite
unique source key on `match_events` and uses `(event_id, match_id)` as the
timeline foreign key. This prevents a timeline row from pairing an event with
another match while retaining `match_id` for efficient match queries.

The projection also enforces one timeline row per match aggregate version and
adds an order index on `(match_id, aggregate_version, event_id)`. Event index
is intentionally not unique: it is command-local and may repeat across
commands, while aggregate version owns match-wide order.

## C1 boundary and correction ownership

Only the current C1 event-type vocabulary is accepted:
`GoalRecorded`, `YellowCardRecorded`, `SecondYellowCardRecorded`,
`RedCardRecorded`, and `SubstitutionRecorded`. Only `first_half` and
`second_half` are accepted because `MatchProjection` rejects non-live C1
actions. Correction markers, verification markers, and voided source events
are resolved by the effective-stream/rebuild boundary; this schema does not
invent a correction column or mutate canonical events.

The table has no foreign key to a match aggregate table because that table is
not persisted yet. The canonical source composite foreign key is the strongest
available identity constraint without fabricating a future match table.

## Deferred fields

- event payload and correction references remain in `match_events`;
- source aggregate/projection freshness versions belong to `PRJ-005`;
- projection rebuild and effective-stream application belong to `PRJ-006`; and
- public REST mapping remains owned by the later public endpoint increments.

No application repository, writer, or public endpoint is added by this
schema-only increment. A later writer must derive rows from the effective
canonical stream and update the projection in the appropriate transaction.

## PostgreSQL invariants

- every timeline event references an existing canonical event from the same
  match;
- one row exists for a given canonical event;
- one row exists for a given match aggregate version;
- aggregate versions are positive and event indexes are non-negative;
- event type is one of the five C1 action types;
- period is a live C1 period; and
- clock milliseconds are non-negative.

## Verification

The ignored clean-database integration test
`match_timeline_projection_schema_enforces_source_order_and_c1_bounds`
applies the complete migration set and verifies canonical source ordering,
metadata round-trip, duplicate event/version rejection, missing-source
rejection, cross-match source rejection, and C1 type/period/clock bounds. It
passed against the dedicated `football_socket_prj003` database. That
disposable database is removed after verification; the persistent project test
database is not used or changed.
