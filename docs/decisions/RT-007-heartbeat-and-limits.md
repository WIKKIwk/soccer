# RT-007 — Realtime connection heartbeat and limits

**Status:** Implemented; heartbeat timing, timeout, monotonic-clock, and connection-limit verification passed  
**Decision date:** 2026-08-09  
**Depends on:** [RT-003 subscribe and unsubscribe contracts](RT-003-subscribe-unsubscribe-contract.md), [RT-004 realtime topic authorization](RT-004-topic-authorization.md), [RT-006 duplicate delivery safety](RT-006-duplicate-delivery-safety.md)

## Decision

Realtime liveness uses native WebSocket Ping/Pong control frames. Heartbeat
frames are not JSON subscription messages and ordinary application traffic does
not silently replace the Pong response.

The initial C1 connection policy is:

```text
max inbound frame       64 KiB
max subscriptions       32 topics per connection
max pending outbound    256 notifications per connection
heartbeat interval      15 seconds
Pong timeout             10 seconds
```

The policy rejects zero limits and requires the Pong timeout not to exceed the
heartbeat interval. The transport adapter checks the frame limit before JSON
parsing, the subscription limit before adding a topic, and the outbound queue
limit before enqueueing a notification. Exceeding a limit is an explicit typed
error; the adapter must not silently grow memory or truncate a frame.

Each connection owns a monotonic-clock heartbeat state. After the interval,
the adapter sends one Ping and waits for its Pong. A timely Pong clears the
outstanding probe. A missing or late Pong produces `CloseUnresponsive`; an
unsolicited Pong is ignored. Backward clock samples are rejected so a wall-clock
adjustment cannot extend a dead connection's lease.

## Implementation

`football-application::realtime_heartbeat` owns the transport-independent
`RealtimeConnectionLimits`, limit checks, heartbeat state, actions, Pong
dispositions, and typed errors. The state machine accepts monotonic elapsed time
in milliseconds supplied by the future WebSocket adapter. It does not open,
close, or write a socket itself.

The repository does not yet contain the public WebSocket server or connection
registry. This increment therefore completes the policy and state boundary that
the later transport adapter will call; connection-count/global rate limiting
remains an operational adapter concern rather than an invented process-global
counter.

## Verification

Application unit tests verify:

- default values and policy validation;
- zero and misordered heartbeat-limit rejection;
- exact frame, subscription, and pending-queue boundaries;
- no Ping before the interval and one Ping at the interval;
- timely Pong acceptance and the next scheduled probe;
- missing/late Pong closure;
- unsolicited Pong ignoring and backward-clock rejection.
