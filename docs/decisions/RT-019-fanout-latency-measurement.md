# RT-019 — Fanout latency measurement boundary

**Status:** Implemented; the current fanout acknowledgement boundary is measured
**Decision date:** 2026-08-09
**Depends on:** [RT-005 match notifications](RT-005-match-notifications.md), [RT-006 duplicate delivery safety](RT-006-duplicate-delivery-safety.md), [RT-017 worker restart duplicate identity](RT-017-worker-restart-duplicate-delivery.md), and [OUT-007 publish completion](OUT-007-publish-completion.md)

## Decision

Until the WebSocket transport and client acknowledgement protocol exist, the
authoritative measurable realtime boundary is:

```text
outbox.created_at -> outbox published_at
```

The metric name is
`outbox_created_to_publish_ack`. `published_at` is the timestamp supplied to
`mark_outbox_published` by the worker after the prepared notification has been
accepted by the current transport boundary and the completion transaction has
been committed.

This is an end-to-end measurement of the currently implemented outbox-to-publish
boundary. It is not a claim about WebSocket network latency or public-client
receipt latency. Client receipt measurement remains a later transport increment
after the WebSocket server and client acknowledgement protocol are implemented.

## Invariants

- The measured message ID must equal the completed outbox message ID.
- `published_at` must not precede `created_at`.
- The result is represented in whole non-negative milliseconds as `u64`.
- Measuring a sample is pure: it does not change delivery state or emit a
  metric by itself.
- The caller invokes the measurement only after the publish-completion
  transaction commits.

## Implementation

`crates/infrastructure/src/realtime_latency.rs` provides the named measurement
constant, validated sample type, explicit error cases, and the pure
`measure_realtime_fanout_latency` function. The public infrastructure export
keeps the boundary available to the future worker/observability adapter without
introducing a metrics backend or a new database table prematurely.

`crates/infrastructure/tests/realtime_notification.rs` verifies the boundary
against a clean PostgreSQL database: it persists and prepares a notification,
commits preparation, records a publish acknowledgement at a known timestamp,
commits completion, and asserts the exact `1,750 ms` sample.

## Verification

The ignored integration test passed against a disposable
`football_socket_rt019` database after applying all repository migrations. The
disposable database was dropped after the test; the persistent project test
database was not changed.
