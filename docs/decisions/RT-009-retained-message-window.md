# RT-009 — Retained-message window

**Status:** Implemented; bounded sequence-and-age window policy verification passed  
**Decision date:** 2026-08-09  
**Depends on:** [RT-002 monotonic per-topic sequence](RT-002-realtime-topic-sequence.md), [RT-008 last-known sequence](RT-008-last-known-sequence.md), [RT-007 realtime connection limits](RT-007-heartbeat-and-limits.md)

## Decision

Reconnect replay is bounded independently for each realtime topic by two
conditions:

```text
maximum retained messages per topic: 256
maximum retained age:                15 minutes
```

A notification is replay-eligible only when both conditions hold:

- its sequence is between the current topic window floor and newest committed
  sequence, inclusive;
- its age is strictly less than 15 minutes. A message at exactly the age
  boundary is expired.

For a newest sequence `N` and count `C`, the sequence floor is:

```text
max(1, N - C + 1)
```

The window is topic-scoped and uses the realtime delivery sequence, never the
canonical aggregate version, event index, outbox attempt count, or UUID order.
The age is supplied by the future storage/transport adapter from its explicit
clock; this policy does not invent a clock or mutate database rows.

RT-009 defines eligibility only. Retained-message storage/eviction, replay of a
valid cursor, gap detection, and `snapshot_required` behavior remain the
following RT-010 through RT-013 increments. The canonical outbox and durable
notification identity are not deleted by this policy.

## Implementation

`football-application::realtime_retention` owns
`RealtimeRetentionPolicy` and `RealtimeRetainedMessageWindow`. The policy
validates positive count/age bounds, computes a positive sequence floor without
underflow, and evaluates the count-and-age intersection. It is transport- and
database-independent so an eventual WebSocket/replay adapter can apply the
same rules.

## Verification

Application unit tests verify:

- default count/age values and policy validation;
- zero-bound rejection;
- early-topic windows that floor at sequence `1`;
- later windows with exact oldest/newest sequence boundaries;
- rejection of sequences outside the window;
- simultaneous sequence and age requirements, including the exact expiry
  boundary.
