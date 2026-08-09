# RT-010 — Realtime sequence gap detection

**Status:** Implemented; retention-boundary, internal-hole, trailing-hole, and client-ahead detection verified  
**Decision date:** 2026-08-09  
**Depends on:** [RT-008 last-known sequence](RT-008-last-known-sequence.md), [RT-009 retained-message window](RT-009-retained-message-window.md), [RT-006 duplicate delivery safety](RT-006-duplicate-delivery-safety.md)

## Decision

Reconnect assessment compares the client's optional `last_known_sequence` with
the topic's retained window and the ordered retained sequence list. It returns
one of:

- `NoCursor` — the client has no previous delivery position;
- `UpToDate` — the cursor equals the newest retained sequence;
- `Replayable` — every sequence from the cursor's successor through the newest
  retained sequence is present and ordered;
- `Gap` — replay cannot be trusted without a later recovery decision.

The typed gap reasons are:

- `BeforeRetention` — the cursor's next sequence is older than the retained
  window floor;
- `InsideRetention` — an expected sequence is absent or a trailing sequence is
  missing even though the cursor is inside the window;
- `ClientAheadOfNewest` — the client cursor is newer than the server's newest
  committed delivery position.

Retained sequence rows must be strictly increasing and inside the declared
window. Unordered, duplicate, or out-of-window storage data is rejected as an
error rather than silently sorted or guessed. This increment detects the gap;
RT-011 owns the `snapshot_required` response and recovery semantics.

## Implementation

`football-application::realtime_gap` owns
`assess_realtime_replay`, `RealtimeReplayAssessment`, and typed gap/storage
errors. It consumes the RT-009 window and an ordered sequence slice supplied by
the future replay adapter. It does not read PostgreSQL, send WebSocket frames,
or decide whether a snapshot is publicly readable.

## Verification

Application unit tests verify no-cursor and up-to-date states, a cursor before
retention, a client-ahead cursor, complete boundary replay, internal and
trailing holes, and rejection of unsorted or out-of-window retained sequences.
