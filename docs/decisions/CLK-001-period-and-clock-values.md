# CLK-001 — Period and clock value objects

**Status:** Accepted  
**Decision date:** 2026-08-08  
**Depends on:** [ARC-004 UTC and timezone rules](ARC-004-utc-timezone-rules.md), [MCH-002 initial match states](MCH-002-initial-match-states.md), [MCH-005 command and event envelopes](MCH-005-command-event-envelopes.md)

## Decision

The initial C1 clock uses two domain value types:

`MatchPeriod` identifies the football period without treating it as a wall
clock. The initial values are `PreMatch`, `FirstHalf`, `HalfTime`, and
`SecondHalf`. First and second half are live periods; pre-match and half-time
are breaks. Extra time and penalty shootout periods are later extensions.

`ClockMilliseconds` represents non-negative football elapsed time in
milliseconds. It is distinct from `UtcInstant`, which remains the source for
event audit timestamps and server clock anchors. Construction from seconds
checks multiplication overflow, and arithmetic checks addition overflow.

This increment defines values only. It does not implement running anchors,
pause/resume, added-time state, correction commands, persistence, or a clock
port. Those behaviors remain in `CLK-002` and `CLK-003`.

## Verification

Database-free domain tests verify period classification, millisecond and
second construction, safe addition, and overflow rejection. No current wall
clock or database is used.
