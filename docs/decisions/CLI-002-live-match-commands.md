# CLI-002 — Live match commands

**Status:** Implemented; parsing, routing, focused tests, and strict lint checks passed
**Decision date:** 2026-08-31
**Depends on:** [CLI-001 configuration and authorization](CLI-001-footballctl-config-auth-adapter.md), [APP-001 command orchestration](APP-001-load-execute-commit-orchestration.md), [APP-002 idempotency and audit](APP-002-expected-version-idempotency-audit.md)

## Decision

`footballctl` now supports the Week 8 operator surface without creating a
second command implementation. Existing `fixture schedule` is retained, and
the adapter adds:

- `lineup save` for immutable draft lineup persistence and read-back;
- match schedule, lineup submit/confirm, first-half start/end, second-half
  start, and finish lifecycle commands;
- regular, penalty, and own goals;
- yellow, second-yellow, and red cards;
- substitutions.

Every canonical match command requires explicit UUIDv7 command,
client-command, event, match, operator, and operator-lease identities together
with expected aggregate version, match clock, and RFC3339 occurrence time.
There are no hidden ID or business-time defaults. The active persisted match
assignment and unexpired operator lease are checked before dispatch.

Both CLI and admin API build the request fingerprint from the same typed domain
command. The shared representation excludes transport syntax and server
recording time, while including every logical identity, concurrency value,
business time, actor, and payload field. A cross-transport retry therefore has
the same idempotency semantics.

The adapter delegates lifecycle and C1 execution to the canonical
infrastructure executors. Those executors own event replay, expected-version
validation, terminal command persistence, audit, projections, and outbox work
inside one transaction.

## Verification

Parser tests cover lineup roles, C1 action distinctions, duplicate flags, and
strict UUID/time values. Subprocess tests prove live commands and lineup save
reach their fail-closed authorization boundaries before any database access.
Application/API/CLI focused tests and strict Clippy checks pass. Database-backed
end-to-end execution remains the explicitly gated `PIL-001` acceptance run.
