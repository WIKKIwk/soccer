# CLK-002 — Clock start, pause, resume, and correction

**Status:** Accepted  
**Decision date:** 2026-08-08  
**Depends on:** [CLK-001 period and clock values](CLK-001-period-and-clock-values.md), [ARC-004 UTC and timezone rules](ARC-004-utc-timezone-rules.md)

## Decision

`MatchClock` stores the clock state described by ARC-004:

- current `MatchPeriod`;
- `base_clock_ms`;
- optional UTC server anchor;
- running flag;
- `added_time_ms` placeholder initialized to zero;
- optional UTC correction timestamp;
- monotonic clock revision.

The current display time while running is:

```text
base_clock_ms + (server_time_utc - anchor_server_time_utc)
```

`pause` materializes that value into `base_clock_ms` and removes the anchor.
`resume` creates a new anchor without changing the base value. `correct`
replaces the base value, records the UTC correction time, and reanchors a
running clock. All operations reject a server time before the active anchor.

The initial valid start is `PreMatch → FirstHalf`; `HalfTime → SecondHalf` is
reserved for the later match-period transition increment. The clock is not
advanced by a process timer and does not call the system clock. Every state
mutation advances the independent clock revision; aggregate version remains
owned by canonical match events.

This increment does not add added-time commands, lifecycle transitions,
injected clock ports, persistence, or API adapters.

## Verification

Database-free domain tests verify anchored elapsed time, pause stability,
resume re-anchoring, correction re-anchoring, UTC correction metadata,
revision advancement, invalid operations, and backward-server-time rejection.
