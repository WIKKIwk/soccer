# RT-018 — Correction during a live match

**Status:** Implemented; live/correction/live realtime ordering verified
**Decision date:** 2026-08-09
**Depends on:** [COR-003 score rebuild after a voided goal](COR-003-score-rebuild-after-voided-goal.md), [RT-005 match notifications](RT-005-match-notifications.md), [RT-006 duplicate delivery safety](RT-006-duplicate-delivery-safety.md), and [RT-014 correction notifications](RT-014-correction-notifications.md)

## Decision

An `EventVoided` correction occurring during an active match does not stop
subsequent live-event delivery. The realtime stream for one match must retain
one strict topic sequence:

```text
1  match.event       GoalRecorded
2  match.correction  EventVoided(goal event)
3  match.event       YellowCardRecorded
```

The correction is a separate wire variant, but it shares the same
`match:{match_id}` topic and sequence allocator as the surrounding live
events. Each outbox row remains independently publishable and completion of
one row does not mark the other rows as published.

## Implementation

`crates/infrastructure/tests/realtime_notification.rs` adds a clean fixture
with three immutable canonical events and three transactional outbox rows for
one match. The test claims and prepares all rows in order, verifies the typed
notification variants and exact IDs/topic/sequences, commits the preparation,
and completes all three publishes.

The fixture preserves the correction target reference and verifies that the
post-correction live event continues through the same fanout boundary. It does
not mutate the original goal event or add a fake WebSocket server.

The existing `correction_execution` fixture remains the source-of-truth test
for canonical append and effective projection replay; RT-018 adds the missing
realtime delivery coverage on top of that boundary.

## Verification

The ignored integration test passed against a disposable
`football_socket_rt018` database after applying all repository migrations. It
verified `match.event → match.correction → match.event`, sequences `1 → 2 → 3`,
three identity rows, and three final published outbox rows. The disposable
database was dropped; the persistent project test database was not changed.
