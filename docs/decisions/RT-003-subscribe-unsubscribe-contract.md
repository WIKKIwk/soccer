# RT-003 — Subscribe and unsubscribe contracts

**Status:** Implemented; typed JSON control frames and canonical topic decoding verified  
**Decision date:** 2026-08-09  
**Depends on:** [RT-001 realtime topic names](RT-001-realtime-topic-names.md), [RT-002 monotonic topic sequence](RT-002-realtime-topic-sequence.md), [ARC-006 API versioning rules](ARC-006-api-versioning.md)

## Decision

The WebSocket control surface uses JSON frames with a client-generated UUID
correlation ID:

```json
{
  "type": "subscribe",
  "request_id": "019ff110-3bc5-7cf0-b3d2-0728c077a930",
  "topic": "match:019ff111-3bc5-7cf0-b3d2-0728c077a930"
}
```

Unsubscription uses the same shape with `"type": "unsubscribe"`. A
successful server result echoes the request ID and topic with `type` set to
`subscribed` or `unsubscribed`.

`request_id` correlates one control request with its acknowledgement. It is
not an authentication credential, authorization proof, or domain idempotency
key. Topic values are decoded through the RT-001 typed vocabulary and must use
the canonical lowercase hyphenated UUID form.

Control frames contain no server-assigned notification sequence, notification
payload, or canonical match state. `Subscribe` may carry the optional client
cursor `last_known_sequence` defined by RT-008; that cursor is not a new
delivery sequence and does not replace authorization or snapshot state.
Authorization decisions and stable rejection responses are defined by RT-004;
connection limits and heartbeat are defined by RT-007. If the cursor cannot be
replayed safely, the server returns the `snapshot_required` recovery response
defined by RT-011 rather than pretending that the subscription is synchronized.

## Implementation

`football-contracts::RealtimeSubscriptionRequest` and
`RealtimeSubscriptionResponse` own the versioned wire shapes. The custom
`RealtimeTopic` serializer keeps the wire representation as one stable topic
string rather than exposing persistence or Rust enum layout.

## Verification

The `football-contracts` tests verify exact subscribe JSON, request/response
round-trip decoding, unsubscribe acknowledgement shape, unsupported topic
rejection, and rejection of noncanonical topic UUID casing.
