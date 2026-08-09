# EVT-001 — Canonical match-event migration

**Status:** Implemented in SQL and clean-database verified on 2026-08-09  
**Decision date:** 2026-08-08  
**Depends on:** [ARC-008 command/event version semantics](ARC-008-command-event-version-semantics.md), [ARC-007 forward-only migrations](ARC-007-forward-only-migrations.md), and [MCH-005 command/event envelopes](MCH-005-command-event-envelopes.md)

## Decision

Migration `0006_match_events.sql` creates the append-only canonical event
storage boundary for the C1 match aggregate. It preserves the event envelope's
identity, causation, deterministic ordering, football time, explicit UTC
timestamps, actor, verification, payload, and correction-reference fields.

`event_id` is the persisted `EventId`. `causation_command_id` references the
internal command row ID, not the caller-provided client command ID. The match
aggregate table is not introduced yet, so `match_id` remains a required UUID
without a fabricated foreign-key target.

The roadmap's earlier `operator_id` label is represented by `actor_id` from the
frozen event-envelope decision. This avoids fabricating a separate operator
identity and allows the same envelope to record supervisor corrections and
verification. The current actor boundary references `admin_users`; role
authorization and the real operator/supervisor assignment remain application
responsibilities.

## Persisted invariants

- The first canonical event version is positive; versions are unique per match.
- `event_index` is zero-based and unique within the causation command.
- Event schema versions are positive, event types are non-blank, and payloads
  are present as `JSONB`.
- Period and verification status use the current C1 domain vocabulary.
- Clock milliseconds are non-negative football time, not a UTC timestamp.
- Correction references point to existing event IDs; source events are not
  updated or deleted by this migration.
- Explicit `occurred_at` and `recorded_at` values keep domain timing separate
  from database/server clock defaults.

No event rows are seeded. No command, match, operator, or supervisor record is
fabricated by this increment.

## Verification

The migration is included in the forward-only SQLx migration sequence and is
statically reviewed against the domain `DomainEventEnvelope` and the required
uniqueness constraints. A real apply check passed through the clean-database
migration fixture, and the match-event integration fixtures applied the same
migration set before inserting and reading canonical event rows. No existing
database was used.
