# ARC-004 — UTC and timezone rules

**Status:** Accepted  
**Decision date:** 2026-08-08  
**Depends on:** [ARC-003 UUID strategy](ARC-003-uuid-strategy.md)

## Canonical time

Every persisted instant uses UTC. This includes:

- command, event, audit, outbox, import, and projection timestamps;
- kickoff instants and scheduling changes;
- event `occurred_at` and `recorded_at`;
- session and lease expiry;
- clock anchors and correction timestamps;
- `generated_at`, `computed_at`, and processing metadata.

PostgreSQL uses `timestamp with time zone` (`timestamptz`) for instants, with the connection/session timezone configured to UTC. A `timestamp without time zone` column must not represent an instant.

API responses serialize instants as ISO-8601 UTC with an explicit `Z` suffix. API input that represents an instant must include an explicit offset or named timezone; an unqualified local date-time is rejected rather than guessed.

## Calendar values versus instants

Calendar-only values are not converted through UTC:

- season start/end dates;
- birth dates;
- local competition dates where no time-of-day is part of the business meaning.

These use an explicit date value. A scheduled kickoff is different: it is an instant, normalized to UTC at the boundary while retaining the source timezone/offset in the scheduling or evidence context when needed for audit.

Named timezones use IANA identifiers such as `Asia/Tashkent`. A numeric offset alone is sufficient only for a single supplied instant; it is not a durable timezone policy.

## Match clock

The football clock is domain elapsed time, not wall-clock time. The stored clock state is:

```text
period
base_clock_ms
anchor_server_time_utc
is_running
added_time_ms
last_corrected_at_utc
version
```

The current display clock is calculated from the stored base value and UTC server anchor through an injected clock port. The database is not updated every second. `PauseClock`, `ResumeClock`, `CorrectClock`, and match interruption are explicit audited actions. A match event carries both football time (`period`, `clock_ms`) and UTC audit/recording timestamps.

## Presentation policy

- Persisted state and public contracts remain UTC and timezone-neutral.
- Presentation timezone belongs to the API/client boundary and is returned as metadata when a client needs it.
- The operational default for O‘zbekiston Superligasi scheduling and operator display is `Asia/Tashkent`.
- Public mobile clients may render dates in the user’s selected timezone, but this never changes the canonical kickoff or event instant.
- Localized date labels are presentation strings and are not stored as authoritative business values.

## Ingress and egress rules

At ingress, the application validates the supplied offset/timezone, resolves the value to one UTC instant, and rejects ambiguous or missing timezone information. At egress, the API emits UTC ISO-8601 values; clients perform local formatting.

External source timestamps are normalized once at ingestion. The original source timestamp, source timezone, and evidence reference may be retained for audit, but no later layer silently reinterprets the instant.

## Testing rules

- Domain tests use an injected fixed UTC clock.
- Tests never depend on the host machine timezone or current wall clock.
- Clock tests cover pause/resume, correction, added time, interruption, and server-anchor changes.
- Date-only values are tested independently from instant conversion.
- Integration tests verify UTC round trips and explicit offset/timezone validation.
- UUIDv7 timestamp bits are never used as the business time source.

## Acceptance criteria

ARC-004 is complete when database and API time contracts distinguish UTC instants, calendar dates, and football elapsed time; no layer guesses a timezone; and operator/public presentation can localize values without changing canonical state.

