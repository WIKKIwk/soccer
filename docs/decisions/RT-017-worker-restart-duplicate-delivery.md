# RT-017 — Worker restart and duplicate delivery

**Status:** Implemented; lease-expiry recovery and duplicate notification identity verified
**Decision date:** 2026-08-09
**Depends on:** [OUT-004 worker claim](OUT-004-worker-claim.md), [OUT-007 commit and publish failures](OUT-007-commit-publish-failures.md), [RT-006 duplicate delivery safety](RT-006-duplicate-delivery-safety.md), and [RT-016 standings revision notifications](RT-016-standings-revision-notifications.md)

## Decision

An outbox worker restart is modelled as an active processing lease expiring
before publish completion. A second worker may reclaim the row, but it must
not create a new realtime identity. The durable identity ledger remains keyed
by the outbox message and preserves:

- the original notification ID;
- the original topic; and
- the original per-topic sequence.

The stale worker cannot acknowledge publish after the second worker owns the
reclaimed lease. Only the current worker may complete publication.

Preparing the same claimed row more than once is a duplicate-delivery path,
not a new event. It must return the same transport notification and leave one
identity row and one topic sequence behind.

## Implementation

`crates/infrastructure/tests/realtime_notification.rs` now composes the public
claim, realtime preparation, identity-ledger, and publish-completion APIs:

1. worker A claims and prepares a match event;
2. the fixture expires worker A's lease;
3. worker B reclaims the row with attempt count `2`;
4. worker B prepares the notification twice and receives the exact same ID,
   topic, sequence, and serialized wire value;
5. worker A's completion is rejected; and
6. worker B completes publish successfully.

No production worker loop, network transport, or canonical event mutation is
added by this test increment.

## Verification

The ignored clean-database integration test passed against a disposable
`football_socket_rt017` database after applying the repository migrations. It
verified one identity row, sequence `1`, attempt count `2`, stale-worker
rejection, and final `published` state. The disposable database was dropped;
the persistent project test database was not changed.
