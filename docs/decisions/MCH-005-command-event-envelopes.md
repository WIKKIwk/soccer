# MCH-005 — Command result and domain event envelopes

**Status:** Accepted  
**Decision date:** 2026-08-08  
**Depends on:** [MCH-004 aggregate version semantics](MCH-004-aggregate-version-semantics.md), [ARC-002 partial event sourcing](ARC-002-partial-event-sourcing.md), [ARC-008 command, event, and version semantics](ARC-008-command-event-version-semantics.md)

## Decision

The match engine defines two transport-independent domain envelopes.

### Command result

`CommandResultEnvelope<R, E>` identifies the internal `CommandId`, the target
`MatchId`, and one terminal `CommandOutcome`:

- `Accepted` carries the final aggregate version, emitted event count, and a
  typed result value;
- `Rejected` carries the aggregate version observed at rejection and a typed
  error value.

Retryable infrastructure failure and command-admission lease state are not
terminal domain results, so they are not represented as an accepted or
rejected outcome here. Client command identity, request fingerprint,
idempotency storage, and retry semantics belong to the later `CMD` increments.

### Domain event

`DomainEventEnvelope<T>` carries an immutable typed payload and the canonical
event metadata that is stable before persistence:

```text
event_id
match_id
aggregate_version
causation_command_id
event_index
event_type
event_schema_version
period
clock_ms
occurred_at
recorded_at
actor
verification_status
payload
supersedes_event_id
voids_event_id
```

`EventIndex` is zero-based per causation command. `EventSchemaVersion` starts
at one. `EventVerificationStatus::Provisional` represents operator live
observation; `Verified` is reserved for supervisor-verified data. Corrections
and voids reference immutable prior events through new envelopes rather than
mutating the original.

`ActorIdentity` is used instead of fabricating a separate operator ID. Role
authorization and the real operator/supervisor assignment remain application
responsibilities.

`period` and `clock_ms` use the typed `MatchPeriod` and
`ClockMilliseconds` values from `CLK-001`. The envelope carries these football
time values but does not calculate a running clock; anchor, pause, resume,
added-time, and correction behavior remain separate clock increments.

## Verification

Database-free domain tests verify accepted and rejected command result shapes,
typed event payloads, causation and zero-based event ordering, schema-version
and event-type validation, timestamps, actor/verification metadata, and
correction references. No command or event is persisted by this increment.
