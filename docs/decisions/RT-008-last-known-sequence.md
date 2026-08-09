# RT-008 — Accept last-known realtime sequence

**Status:** Implemented; positive cursor decoding and backward-compatible subscribe wire verification passed  
**Decision date:** 2026-08-09  
**Depends on:** [RT-002 monotonic per-topic sequence](RT-002-realtime-topic-sequence.md), [RT-003 subscribe and unsubscribe contracts](RT-003-subscribe-unsubscribe-contract.md), [RT-006 duplicate delivery safety](RT-006-duplicate-delivery-safety.md)

## Decision

`Subscribe` accepts an optional client cursor:

```json
{
  "type": "subscribe",
  "request_id": "019ff130-3bc5-7cf0-b3d2-0728c077a930",
  "topic": "match:019ff131-3bc5-7cf0-b3d2-0728c077a930",
  "last_known_sequence": 7
}
```

The cursor is the last server-assigned sequence the client observed for that
topic. It is not the canonical match aggregate version, event index, outbox
attempt count, or a permission proof. A missing cursor means the client has no
known delivery position. Zero, negative, fractional, or non-integer cursors are
rejected by the typed `RealtimeSequence` decoder.

The field is optional and omitted from serialization when absent, so the RT-003
subscribe JSON shape remains valid for existing clients. RT-008 only accepts
and preserves the cursor; retained-message lookup, replay-window decisions, gap
detection, and `snapshot_required` responses remain RT-009 through RT-013.

## Implementation

`football-contracts::RealtimeSequence` now deserializes only positive unsigned
integers. `RealtimeSubscriptionRequest::Subscribe` carries
`last_known_sequence: Option<RealtimeSequence>` with a serde default for old
frames. No transport handler or replay store is invented before the retained
message contract exists.

## Verification

The contracts tests verify:

- a positive cursor serializes as `last_known_sequence` and round-trips;
- an old subscribe frame without the field still decodes and serializes
  unchanged;
- a zero cursor is rejected;
- existing topic, notification, subscribe, and unsubscribe contracts remain
  valid.
