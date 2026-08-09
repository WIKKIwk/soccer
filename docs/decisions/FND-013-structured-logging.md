# FND-013 — Structured logging

**Status:** Accepted  
**Decision date:** 2026-08-08  
**Depends on:** [FND-008 typed process configuration](FND-008-typed-configuration.md), [FND-010 secret redaction](FND-010-secret-redaction.md)

## Decision

`football-platform::observability` owns one process-wide `tracing` subscriber.
It accepts the typed `RUST_LOG` filter from `ApplicationConfig`, rejects empty
or malformed filters, and emits JSON structured records through
`tracing-subscriber`.

The admin API initializes the subscriber once at process startup and emits its
startup record as structured fields (`service`, `bind_address`, and `route`).
Handlers do not initialize subscribers or print unstructured startup records.
The default filter is `info`; target-specific filters remain configurable via
`RUST_LOG`.

The current increment does not add request IDs, actor IDs, match IDs, or HTTP
latency fields. Those are separate observability increments and must be added
as fields at the appropriate request/command boundaries.

## Safety

Startup logging includes no database URL, credentials, command payload, or
database error internals. Configuration debug output remains governed by
FND-010. Invalid filter configuration fails process startup clearly instead of
silently changing the requested filter.

## Verification

Platform unit tests cover valid target filters, empty filters, and malformed
filters. The admin API process smoke test must show a JSON startup record and
continue to serve its existing admin info route.
