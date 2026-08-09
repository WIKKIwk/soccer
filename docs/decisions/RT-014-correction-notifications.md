# RT-014 — Correction notifications

**Status:** Implemented; claimed correction outbox rows have a dedicated typed realtime wire contract
**Decision date:** 2026-08-09
**Depends on:** [RT-005 match notifications](RT-005-match-notifications.md), [RT-006 duplicate delivery safety](RT-006-duplicate-delivery-safety.md), and [COR-001 correction reason and void event](COR-001-correction-reason-and-void-event.md)

## Decision

Correction events use the existing transactional outbox message type
`match.event`, because the outbox stores the canonical event envelope. The
realtime preparation boundary classifies the envelope by its canonical
`event_type` and does not expose an `EventVoided` row as an ordinary live C1
notification.

The notification variants are:

- a C1 event becomes `type = match.event` and retains the existing versioned
  match-event envelope payload;
- an `EventVoided` correction becomes `type = match.correction` with schema
  version `1`, the match topic, the durable topic sequence, the correction
  event ID, the voided event ID, and the exact nonblank correction reason.

Both variants share the same `match:{match_id}` topic and monotonically
allocated sequence. `notification_id` remains the durable outbox message
identity. A correction retry therefore reuses the same notification identity,
topic, and sequence through the RT-006 identity ledger.

Correction preparation fails closed when the event schema is unsupported, the
void target is missing or self-referential, a replacement reference is
present, or the encoded reason is absent or blank. The C1-specific builder
rejects correction rows, and the correction-specific builder rejects C1 rows;
the generic builder returns the typed `RealtimeNotification` union for a
future worker/fanout adapter.

## Implementation

`football-contracts::RealtimeMatchCorrectionNotification` owns the stable
`match.correction` wire shape. `football-contracts::RealtimeNotification`
contains the live and correction variants without adding a transport wrapper.

`football-infrastructure::build_realtime_notification` classifies a claimed
`StoredOutboxMessage`, while
`prepare_realtime_notification` performs the same classification after
locking the claim and allocating/reusing the durable topic identity in the
caller-owned transaction. Typed live and correction preparation helpers remain
available for callers that want an explicit variant.

This increment does not add a WebSocket server or a network fanout loop. The
low-level correction executor still returns an uncommitted canonical stream;
the higher-level correction command transaction/outbox orchestration remains a
separate boundary. This decision covers the publish-ready contract once a
correction outbox row is claimed.

## Verification

Database-free contract and infrastructure tests verify the exact correction
wire JSON, union routing, stable topic/sequence/identity fields, rejection of
corrections by the live-event builder, and fail-closed handling of blank
reasons and self-referential targets. No migration was required; the existing
outbox and realtime identity schema are unchanged.
