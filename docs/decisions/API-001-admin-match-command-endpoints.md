# API-001 — Admin match command endpoints

**Status:** Implemented; route, contract, validation, and strict lint checks passed
**Decision date:** 2026-08-31
**Depends on:** [APP-001 command orchestration](APP-001-load-execute-commit-orchestration.md), [APP-002 idempotency and audit](APP-002-expected-version-idempotency-audit.md), [CLI-001 development authorization](CLI-001-footballctl-config-auth-adapter.md)

## Decision

The versioned admin API now exposes regular-time canonical commands for match
scheduling, lineup submission and confirmation, both period transitions,
finish, goals, all three C1 card meanings, and substitutions. Each request
carries explicit command, client-command, event, operator-lease, expected
aggregate version, match clock, and occurrence-time values. UUID inputs are
canonical UUIDv7 values and occurrence time is explicit RFC3339.

All handlers call the same application admission and PostgreSQL execution
ports used by the canonical command core. They do not write events,
projections, audits, outbox messages, or terminal results independently.
Accepted and deterministic rejected commands use one versioned response shape,
including persisted replay of a prior terminal result.

The Week 8 API uses a fail-closed development identity adapter: the exact
`x-development-authorization: 1` guard and a UUIDv7 development administrator
header are required, then the adapter verifies the persisted administrator is
active, assigned to the match, and owns the supplied unexpired operator lease.
This is controlled development identity injection, not production
authentication. Login, sessions, rotation, and persistent RBAC remain owned by
the later IDN roadmap increments.

Lineup submission loads the immutable persisted draft and verifies that its
fixture owns the match assignment before constructing the canonical snapshot.
Live actions carry an explicit first- or second-half period; the canonical C1
executor rechecks it against lifecycle replay and expected aggregate version.

## Verification

Admin API tests cover exact development headers, fail-closed command routing,
stable malformed-JSON errors, and all OpenAPI route/contract names without a
database. Contract serialization tests cover strict request fields, card
meaning, and terminal response shape. The workspace command core tests cover
lifecycle/C1 reduction and transaction construction; the clean-database
lifecycle fixture remains explicitly gated and is not run implicitly by HTTP
tests. The complete deployed database-backed operator flow belongs to
`PIL-001`.
