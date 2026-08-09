# CLK-003 — Injected clock port

**Status:** Accepted  
**Decision date:** 2026-08-08  
**Depends on:** [CLK-002 clock operations](CLK-002-clock-operations.md), [ARC-004 UTC and timezone rules](ARC-004-utc-timezone-rules.md), [ARC-001 modular-monolith boundary](ARC-001-modular-monolith-boundary.md)

## Decision

The application boundary owns `ClockPort`, whose only responsibility is to
return the current UTC server instant. `MatchClockCoordinator<C>` receives an
implementation of that port and supplies its result to the pure domain
`MatchClock` operations for current display time, start, pause, resume, and
correction.

The domain never calls `SystemTime`, `OffsetDateTime::now_utc`, or another
process/global clock. A production adapter can implement `ClockPort` later;
tests use a fixed mutable value. This keeps clock behavior reproducible and
makes host timezone and test execution time irrelevant.

The port does not own clock state, persistence, retries, or authorization. It
only supplies the UTC instant at the application boundary.

## Verification

Application tests use a fixed injected clock, advance it explicitly, and
verify that display, pause, resume, and correction all use that value. No
system clock or external service is accessed.
