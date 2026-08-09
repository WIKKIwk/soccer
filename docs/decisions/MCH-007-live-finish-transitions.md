# MCH-007 — Live-period and finish transitions

**Status:** Accepted  
**Decision date:** 2026-08-08  
**Depends on:** [MCH-006 pre-match and lineup transitions](MCH-006-pre-match-lineup-transitions.md), [MCH-002 initial match states](MCH-002-initial-match-states.md), [CLK-002 clock operations](CLK-002-clock-operations.md)

## Decision

`MatchAggregate` exposes named lifecycle operations for the regular C1 live
path:

```text
LineupsConfirmed
  → start_first_half
  → end_first_half
  → start_second_half
  → finish
```

Each operation delegates to the typed lifecycle transition rule and rejects
skipped or out-of-order operations. This increment covers regular time only;
extra time, penalty shootout, delayed, abandoned, and awarded outcomes remain
future state decisions.

The aggregate lifecycle operations do not directly mutate `MatchClock` or
advance aggregate version yet. Clock timing is an explicitly injected domain
operation, and aggregate version advances only when accepted canonical events
are emitted and committed by later command/event increments.

## Verification

Database-free aggregate tests run the complete path from confirmed lineups to
`Finished`, preserve the initial aggregate version before event emission, and
reject finishing from `FirstHalf`.
