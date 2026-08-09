# RT-015 — Verification change notifications

**Status:** Implemented; claimed verification-event outbox rows have a dedicated typed realtime wire contract
**Decision date:** 2026-08-09
**Depends on:** [RT-005 match notifications](RT-005-match-notifications.md), [RT-006 duplicate delivery safety](RT-006-duplicate-delivery-safety.md), [RT-014 correction notifications](RT-014-correction-notifications.md), and [P0-006 initial C1 commands and canonical events](P0-006-c1-commands-events.md)

## Decision

The frozen canonical verification event vocabulary is `MatchVerified` and
`MatchReopened`. A future verification command will continue to use the
existing `match.event` transactional outbox envelope because the outbox is
keyed by canonical match events.

The realtime preparation boundary classifies those two event types and emits a
dedicated `type = match.verification` notification with schema version `1`.
The notification contains:

- the stable notification ID, match topic, and durable per-topic sequence;
- the match ID and canonical verification event ID;
- an explicit `change_type` of `match_verified` or `match_reopened`;
- the exact canonical envelope `verification_status`, parsed only from the
  existing `provisional|verified` vocabulary; and
- the unchanged versioned event envelope payload for verification-specific
  fields and future evidence metadata.

Verification notifications share the match topic and identity ledger with C1
and correction notifications. A retry therefore reuses the same notification
ID, topic, and sequence. The generic builder routes the variant, while the
C1-specific builder rejects verification rows so a verification decision cannot
be interpreted as a live football action.

Missing or unknown `verification_status` fails closed. No previous status is
invented: the current canonical event status is the only status this increment
can verify from persisted data.

## Implementation

`football-contracts::RealtimeVerificationChangeType` freezes the two canonical
event names and their snake-case wire names.
`football-contracts::RealtimeMatchVerificationNotification` owns the
`match.verification` wire shape, and `RealtimeNotification` now includes its
verification variant.

`football-infrastructure::build_realtime_notification` and
`prepare_realtime_notification` classify claimed outbox rows. Explicit typed
build and preparation helpers are also available for verification callers.

This increment does not create a match verification command, authorization
policy, evidence table, or producer. The match-level state model is owned by
`QUA-001`; command, authorization, evidence, audit, and producer integration
remain in `QUA-002` through `QUA-006` and `SRC-001`. The RT seam is
publish-ready once those increments persist a canonical `MatchVerified` or
`MatchReopened` event and its outbox row.

## Verification

Contract tests verify the exact `match.verification` JSON, event-kind mapping,
status mapping, and union serialization. Infrastructure tests verify generic
routing, explicit-builder rejection of live events, both verification event
kinds, and fail-closed unknown status handling. A clean disposable PostgreSQL
fixture claimed a `MatchVerified` outbox row and verified notification type,
sequence `1`, canonical identity, status, and the durable identity ledger.
The disposable database was removed; the persistent project test database was
not changed.
