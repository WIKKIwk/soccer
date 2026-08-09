# RT-002 — Monotonic per-topic realtime sequence

**Status:** Implemented; clean-database transactional and concurrent sequence verification passed  
**Decision date:** 2026-08-09  
**Depends on:** [RT-001 realtime topic names](RT-001-realtime-topic-names.md), [ARC-003 UUID strategy](ARC-003-uuid-strategy.md), [ARC-009 projection boundaries](ARC-009-projection-boundaries.md)

## Decision

Each realtime topic has an independent positive sequence beginning at `1`.
The sequence is a delivery cursor, not a canonical match aggregate version,
event index, UUID ordering, outbox attempt count, or database migration
version.

The durable allocator stores the last committed sequence in
`realtime_topic_sequences`. Allocation runs inside the caller's PostgreSQL
transaction using an insert-or-increment operation keyed by the stable topic
name. PostgreSQL serializes concurrent allocations for the same topic on its
primary key; different topics do not share a counter. A rollback does not
commit the allocation, so the next committed allocation can reuse that number.

This increment does not publish a notification or define retention. The later
publish contract must bind the allocated sequence to a notification before
transport delivery; reconnect and gap behavior remain RT-008 through RT-013.

## Implementation

`football-contracts::RealtimeSequence` rejects zero and serializes as a
positive integer. `football-infrastructure::allocate_realtime_topic_sequence`
allocates the next value transactionally, while
`load_realtime_topic_sequence` reads the committed cursor for diagnostics and
future synchronization work.

## Verification

The clean PostgreSQL `realtime_sequence` fixture applied migrations 1–14 and
verified first sequence `1`, concurrent same-topic allocation ending at `2`,
rollback without advancing the committed cursor, the next committed sequence
`3`, independent unseen topics, and rejection of invalid stored sequence `0`.
