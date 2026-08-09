# RT-016 — Standings revision notifications

**Status:** Implemented; typed season topic and source-revision notification seam verified
**Decision date:** 2026-08-09
**Depends on:** [RT-002 realtime topic sequence](RT-002-realtime-topic-sequence.md), [RT-003 subscribe and unsubscribe contract](RT-003-subscribe-unsubscribe-contract.md), [RT-004 topic authorization](RT-004-topic-authorization.md), [RT-009 retained message window](RT-009-retained-message-window.md), and [ARC-009 projection boundaries](ARC-009-projection-boundaries.md)

## Decision

Standings revision changes use a season-scoped realtime topic:

```text
standings:{season_id}
```

The notification wire contract is `type = standings.revision` with schema
version `1`. It contains:

- the stable notification ID, standings topic, and per-topic delivery sequence;
- the season ID; and
- `source_results_revision`, the positive post-increment revision owned by the
  season's verified-result boundary.

`source_results_revision` is intentionally not called a computed standings
revision. The event tells a client that canonical verified-result freshness
advanced; it does not claim that the asynchronous standings projection has
already processed that revision. The client reads the REST standings resource
for `standings_revision`, `source_results_revision`, `computed_at`, and
`is_rebuilding` metadata.

The topic uses the existing generic subscription, authorization, sequence,
retention, gap, and snapshot-required contracts. Public subscription remains
fail-closed on the authoritative `public_readable` fact; this increment does
not infer competition or season access from an ID.

## Implementation

`football-contracts::RealtimeTopic` now owns the canonical standings topic
parser/serializer. `RealtimeStandingsRevisionNotification` and
`RealtimeStandingsResultsRevision` own the versioned wire fields and reject a
zero revision for a change notification.

`football-application::build_realtime_standings_revision_notification` derives
the topic from the supplied season ID and builds the typed notification. This
is the publish-ready contract seam for the future standings-result outbox or
projection worker.

No match-event outbox row is reinterpreted as a standings message. The current
outbox schema is match-aggregate-specific, and the standings producer,
`results_revision` persistence, `computed_results_revision` read model, and
REST standings endpoint belong to the pending `STD-*`/`PUB-*` increments.

## Verification

Contract tests verify exact standings topic formatting, canonical UUID parsing,
subscription serialization, the exact `standings.revision` JSON, notification
union serialization, and zero-revision rejection. Application tests verify
topic derivation, source revision preservation, and fail-closed invalid input.
Authorization tests verify public standings access uses the same authoritative
readability fact as match topics.

No database migration or real standings data write was performed in this
increment.
