# RT-011 — Snapshot-required response

**Status:** Implemented; typed response serialization and all RT-010 gap mappings verified  
**Decision date:** 2026-08-09  
**Depends on:** [RT-003 subscribe and unsubscribe contracts](RT-003-subscribe-unsubscribe-contract.md), [RT-008 last-known sequence](RT-008-last-known-sequence.md), [RT-010 sequence gap detection](RT-010-sequence-gap-detection.md)

## Decision

When a subscribe/reconnect request has a typed RT-010 `Gap`, the server returns
a control response rather than acknowledging a synchronized subscription or
inventing missing notifications:

```json
{
  "type": "snapshot_required",
  "request_id": "019ff150-3bc5-7cf0-b3d2-0728c077a930",
  "topic": "match:019ff151-3bc5-7cf0-b3d2-0728c077a930",
  "reason": "before_retention"
}
```

`request_id` and `topic` are echoed from the request. The stable reason values
are:

- `before_retention` — the cursor is older than the retained sequence floor;
- `inside_retention` — retained data has an internal or trailing sequence hole;
- `client_ahead_of_newest` — the client cursor is newer than the server's
  newest committed realtime sequence.

The response contains no snapshot payload, REST URL, canonical aggregate
version, or guessed sequence. It is a recovery signal, not a success
acknowledgement. RT-012 owns integration with the canonical REST snapshot and
the synchronization boundary after that fetch. A retained-storage validation
error is not converted into `snapshot_required`; it remains a fail-closed
application/storage error.

## Implementation

`football-contracts::RealtimeSubscriptionResponse::SnapshotRequired` owns the
wire shape and `RealtimeSnapshotRequiredReason` owns its stable reason
vocabulary. `football-application::realtime_snapshot` maps every RT-010 gap to
that response without reading PostgreSQL or sending a frame.

## Verification

Contracts tests verify the exact `snapshot_required` JSON shape, round-trip
decoding, and all reason names. Application tests verify the mappings for
retention-boundary, internal/trailing, and client-ahead gaps.
