# RT-005 — Publish match notifications

**Status:** Implemented; claimed-outbox preparation, transactional sequence binding, rollback, and wire-shape verification passed  
**Decision date:** 2026-08-09  
**Depends on:** [OUT-004 lease-backed worker claim](OUT-004-worker-claim.md), [OUT-007 commit and publish failures](OUT-007-commit-publish-failures.md), [RT-001 realtime topic names](RT-001-realtime-topic-names.md), [RT-002 realtime topic sequence](RT-002-realtime-topic-sequence.md), [ARC-009 projection boundaries](ARC-009-projection-boundaries.md)

## Decision

One worker-owned `processing` outbox row with `message_type = match.event` and
`schema_version = 1` can be prepared as one versioned realtime match
notification. Preparation is transport-independent and uses these stable
bindings:

- `notification_id` is the outbox message/source event identity;
- `topic` is `match:{aggregate_id}`;
- `sequence` is the next durable sequence for that topic;
- `payload` is the existing versioned match-event integration envelope;
- the notification wire type is `match.event` with notification schema version
  `1`.

The preparation boundary validates that the message is actively claimed, is a
`match.event` message with the supported outbox schema, and has matching
`match_id`, `event_id`, nonblank event type, and positive event schema metadata
in its payload. A pending, published, poisoned, unsupported, or identity-
mismatched row is not converted into a notification.

Topic-sequence allocation and notification preparation run in the caller's
transaction. The caller must commit that transaction before sending bytes to a
WebSocket connection. A transport acknowledgement is a separate step and must
still pass through the existing worker-owned `mark_outbox_published` boundary;
preparation alone never marks the outbox row as published.

This increment deliberately does not add an Axum WebSocket server or connection
fanout loop. The repository has no public realtime transport yet, and the
roadmap places WebSocket delivery after the minimal REST vertical slice. The
implemented boundary is therefore the smallest complete publish-ready contract
for the future transport adapter. Retry identity and client-side duplicate
handling remain RT-006 work.

## Implementation

`football-contracts::RealtimeMatchNotification` owns the outbound versioned
wire shape. `football-infrastructure::prepare_realtime_match_notification`
validates a claimed `StoredOutboxMessage`, allocates its topic sequence, and
returns the transport-ready notification without performing network I/O.

## Verification

The dedicated clean PostgreSQL `realtime_notification` fixture applied
migrations 1–14 and verified:

- preparation and sequence allocation roll back together with the outbox
  claim;
- a committed claim remains `processing` and retains its worker lease;
- the first committed notification uses sequence `1`, the canonical match
  topic, the source event identity, and the unchanged event envelope payload;
- serialized output contains `type = match.event`, schema version `1`, the
  canonical topic, and sequence `1`.

The fixture database was dropped after verification. Persistent project test
database state was not changed.
