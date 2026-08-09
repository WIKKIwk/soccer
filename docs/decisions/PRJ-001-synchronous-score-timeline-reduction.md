# PRJ-001 — Synchronous score and timeline reduction

**Status:** Implemented as a database-independent domain projection  
**Decision date:** 2026-08-09  
**Depends on:** [EVT-006 C1 live event payloads](EVT-006-c1-live-event-payloads.md), [EVT-004 event stream append semantics](EVT-004-append-multiple-events.md), and [ARC-009 projection boundaries](ARC-009-projection-boundaries.md)

## Decision

`MatchProjection` reduces an already ordered `CanonicalEventStream<C1EventPayload>`
into the first synchronous read state:

- `MatchScore` with non-negative home and away goal counters;
- the current aggregate version; and
- an ordered `TimelineEntry` list containing event ID, aggregate version,
  command-local event index, C1 event type, period, and match clock.

The canonical event stream remains authoritative. The projection does not
accept an operator-provided total and does not expose a write path that can
overwrite the event stream.

Before state is changed, each event is checked for:

- the projection's match ID;
- the next contiguous aggregate version;
- a unique event ID;
- agreement between typed payload event type and envelope event type;
- the current event schema version;
- a live football period for every C1 action event; and
- a goal team that belongs to the fixture.

Only `GoalRecorded` changes the score. The payload's `team_id` already means
the team credited with the score, so regular, penalty, and own-goal events all
increment that credited side. For an own goal, command validation must encode
the opponent as the credited side; the reducer does not flip the score a
second time. Cards and substitutions are preserved in the timeline but do not
yet derive player eligibility or card state.

## Deferred rules

Lineup membership, players-on-the-pitch eligibility, second-yellow sending-off
state, event correction/void semantics, and persisted synchronous snapshots are
later aggregate or projection increments. This reducer consumes the effective
ordered C1 stream supplied by those boundaries; it does not guess those rules
from incomplete data.

## Verification

Database-free domain tests cover score reduction for regular, penalty, and
own-goal classifications, non-scoring card/substitution timeline entries,
live-period, match/team/type/schema/version invariants,
no-mutation-on-rejection behavior, and deterministic replay of the same
canonical stream.
