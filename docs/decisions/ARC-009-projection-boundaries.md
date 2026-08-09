# ARC-009 — Synchronous and asynchronous projection boundaries

**Status:** Accepted  
**Decision date:** 2026-08-08  
**Depends on:** [ARC-002 partial event sourcing](ARC-002-partial-event-sourcing.md) and [ARC-008 command/event/version semantics](ARC-008-command-event-version-semantics.md)

## Authoritative source

The match event stream is authoritative for match behavior. Every projection, snapshot, cache, notification, and search document is disposable derived state. No projection may accept an operator total or manually overwrite a value that the canonical stream can reduce.

## Synchronous command-transaction state

The following commit in the same domain transaction as the accepted command:

- canonical match events;
- current match lifecycle status;
- current score;
- current clock state;
- players on the pitch;
- current aggregate version;
- minimal timeline row or timeline head;
- audit entry;
- outbox messages.

This state is required for immediate command validation, conflict detection, operational recovery, and the first REST match snapshot. It is a rebuildable read optimization, not a second authority. The transaction records the source aggregate version used to produce each current-state value.

The first vertical slice may read this transactionally updated match snapshot through a public query adapter. The adapter remains read-only and must not expose SQL rows as public contracts.

## Asynchronous projections

The following are downstream projections and may lag or rebuild:

- season standings;
- top-scorer and discipline leaderboards;
- competition form;
- aggregated player and team statistics;
- quality aggregates and review queues;
- notifications and realtime fanout;
- search documents;
- future analytics and feature extraction.

They consume committed outbox/integration messages or deterministic event-stream rebuilds. They never write back into the match aggregate or canonical event stream.

## Projection metadata and freshness

Every non-authoritative projection records enough source metadata to explain its state:

```text
source_aggregate_version or source_results_revision
computed_at_utc
projection_version
is_rebuilding
is_current
last_error_code (when safe to expose)
```

For standings, `results_revision` is incremented with the verified/corrected result and `computed_results_revision` identifies the last processed revision. Public responses expose freshness so a client cannot mistake a lagging table for current canonical truth.

Current match state is expected to be transactionally current for the command commit. If a rebuilt match projection is temporarily behind, the API either reads the current synchronized snapshot or discloses the source-version lag; it never silently combines incompatible versions.

## Outbox and delivery

Outbox insertion is synchronous with canonical events and critical state. Delivery is asynchronous after commit:

1. worker claims a committed outbox message safely;
2. worker publishes or applies it with retry/backoff;
3. consumer records its processed source/version or applies an idempotent upsert;
4. failures remain observable and retryable without changing canonical state.

No WebSocket, notification, search, or external integration message is published before the source transaction commits. Duplicate delivery is safe. A worker restart cannot create a second canonical match event.

## Rebuild and correction behavior

Any projection can be dropped and rebuilt from the effective canonical event stream or verified result revision. Corrections and voids invalidate derived values through normal reduction; they do not require mutating historical source events.

Projection rebuilds run against a known source version, compare the resulting values with consistency checks, and publish the new projection version only after successful completion. A rebuild failure leaves the previous projection state and exposes rebuilding/error metadata rather than publishing partial truth.

## Client synchronization

REST provides canonical current snapshots. WebSocket delivers versioned notifications or patches after outbox delivery. A client that detects a sequence gap, unsupported event schema, or projection mismatch fetches a REST snapshot; it does not attempt to infer missing football state locally.

## Explicit non-goals

- making standings or statistics part of the match command transaction;
- treating a WebSocket message, cache, search index, or log as authoritative;
- adding a broker solely to move the first synchronous state;
- hiding projection lag from public clients;
- allowing a projection worker to repair canonical state by direct table writes.

## Acceptance criteria

ARC-009 is complete when every important value has one declared owner, synchronous match state and asynchronous cross-match projections have explicit consistency semantics, outbox delivery is post-commit and idempotent, and all projections can be rebuilt from canonical source data.

