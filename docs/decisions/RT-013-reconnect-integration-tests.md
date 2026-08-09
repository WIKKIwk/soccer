# RT-013 — Reconnect integration tests

**Status:** Implemented; reconnect recovery chain verified through public crate APIs  
**Decision date:** 2026-08-09  
**Depends on:** [RT-010 sequence gap detection](RT-010-sequence-gap-detection.md), [RT-011 snapshot-required response](RT-011-snapshot-required-response.md), [RT-012 canonical REST snapshot](RT-012-canonical-rest-snapshot.md)

## Decision

The reconnect integration boundary is tested without inventing an Axum
WebSocket server. The test composes the same public application and contract
APIs that a future transport adapter will call:

```text
last_known_sequence
  → retained-window assessment
  → typed gap
  → snapshot_required response
  → canonical MatchProjection REST snapshot
```

The recovery contract is:

- a cursor before retention returns `BeforeRetention` and a
  `snapshot_required` response;
- a complete retained sequence range returns `Replayable` and does not request
  a snapshot;
- malformed retained ordering fails closed before a snapshot response is
  constructed; and
- the recovery snapshot is reduced from a canonical event envelope, preserving
  its score, aggregate version, event identity, timeline, and verification
  metadata.

The test remains transport-independent because the public WebSocket server is
not yet in the repository and the REST HTTP route belongs to `PUB-001`. It
therefore verifies the application/contract seam rather than a fake network
implementation.

## Implementation

`crates/application/tests/realtime_reconnect.rs` creates one typed canonical
goal event, reduces it into `MatchProjection`, exercises stale and complete
reconnect cursors, maps a stale cursor through
`realtime_snapshot_required_response`, and builds the canonical REST snapshot
with `build_canonical_match_snapshot`.

## Verification

The integration test verifies the exact `snapshot_required` JSON response,
canonical projection score/timeline preservation, replay without snapshot for a
complete retained range, and fail-closed handling of unordered retained rows.
