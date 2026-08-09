# RT-006 — Duplicate delivery safety

**Status:** Implemented; retry identity reuse, repeated preparation, sequence stability, and publish completion verified  
**Decision date:** 2026-08-09  
**Depends on:** [RT-005 match notifications](RT-005-match-notifications.md), [OUT-005 deterministic retry and backoff](OUT-005-retry-backoff.md), [OUT-007 commit and publish failures](OUT-007-commit-publish-failures.md), [ARC-009 projection boundaries](ARC-009-projection-boundaries.md)

## Decision

One outbox message represents one realtime notification identity for its entire
delivery lifecycle. The durable identity is:

```text
message_id -> (topic, sequence)
```

The outbound `notification_id` is the same stable `message_id`. A retry after a
transport timeout or transient failure must reuse the original notification ID,
topic, sequence, and source outbox payload. A consumer can therefore safely
ignore a notification it has already applied using the notification identity;
it never treats a redelivery as a new canonical match event or a new command.

The first preparation locks the current outbox claim, allocates one topic
sequence, and inserts the mapping into
`realtime_notification_identities`. Later preparations load the existing
mapping instead of advancing `realtime_topic_sequences`. The primary key on
`message_id` prevents one message from receiving two identities, and the unique
`(topic, sequence)` constraint prevents two messages from occupying one topic
position. A malformed or topic-mismatched stored identity fails closed.

Identity creation is in the same transaction as the outbox claim and sequence
allocation. A rollback leaves no identity or sequence cursor advance. The
identity is retained after publish completion so later duplicate acknowledgements
or transport redeliveries have a stable source record; retention cleanup remains
a later synchronization/retention contract.

## Implementation

Migration `0015_realtime_notification_identities.sql` adds the durable mapping.
`football-infrastructure::prepare_realtime_match_notification` serializes
preparation on the outbox row, creates the mapping once, and reuses it for every
retry. `RealtimeMatchNotification` documents `notification_id` as the consumer
deduplication key. Publish completion still uses the existing worker-owned
`mark_outbox_published` boundary and does not create another notification.

## Verification

The dedicated clean PostgreSQL `realtime_duplicate` fixture applied migrations
1–15 and verified:

- a preparation rollback leaves the outbox pending with no identity or sequence;
- the initial committed delivery gets sequence `1`;
- a transient failure and due retry produce an identical notification object;
- repeated preparation within one retry transaction is idempotent;
- the topic cursor remains at `1`, the identity ledger has exactly one row, and
  the second attempt completes the outbox as `published` with attempt count `2`.

The fixture database was dropped after verification. The persistent project
test database and other databases were not modified.
