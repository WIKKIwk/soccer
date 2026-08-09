# EVT-002 — Event schema versioning

**Status:** Implemented as a database-independent domain boundary  
**Decision date:** 2026-08-08  
**Depends on:** [EVT-001 canonical match-event migration](EVT-001-canonical-match-event-migration.md), [ARC-006 API versioning](ARC-006-api-versioning.md), and [ARC-007 forward-only migrations](ARC-007-forward-only-migrations.md)

## Decision

Canonical event schema versions are scoped by `event_type`. The domain now
provides an `EventSchemaRegistry` with one current schema definition per event
type and an explicit list of historical versions that remain readable.

Resolving a persisted event has three possible outcomes:

- the persisted version is the current version and can be reduced directly;
- the persisted version is readable but older and requires an explicit upcast
  to the current version;
- the event type or version is unsupported and resolution fails.

The registry never guesses a payload transformation. Actual typed payloads and
their upcasters are added with the C1 event definitions and stream loader. An
old event's meaning is never changed in place.

## Invariants

- Every definition has exactly one current positive version.
- The current version must be in the readable-version set.
- A readable version is listed once only.
- One event type cannot be registered twice with competing current schemas.
- Unknown event types and unsupported versions are explicit errors, not
  fallback-to-latest behavior.
- An older readable schema produces `RequiresUpcast` so replay cannot silently
  feed an unconverted payload to the reducer.

## Verification

Database-free domain tests cover current-version resolution, historical-version
resolution, unknown type/version rejection, invalid definition construction,
and duplicate registration. No payload is upcast or persisted by this
increment; those operations remain owned by later typed-event and stream
increments.
