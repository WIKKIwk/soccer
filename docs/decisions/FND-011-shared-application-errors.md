# FND-011 — Shared application errors

**Status:** Accepted  
**Decision date:** 2026-08-08  
**Depends on:** [ARC-005 error taxonomy](ARC-005-error-taxonomy.md)

## Decision

`football-platform::errors` owns the transport-neutral application failure
model used at process boundaries. It defines:

- the stable error categories from ARC-005;
- the initial stable machine-readable `ErrorCode` values;
- category and retryability lookup for each code; and
- `ApplicationError` with a safe message and string metadata details.

The application error does not contain HTTP status codes. HTTP and CLI adapters
map it at their own boundaries in later increments. Domain code remains free of
HTTP and transport concerns.

The initial code set follows the ARC-005 examples, including request,
authentication, authorization, resource, validation, domain, conflict,
idempotency, dependency, rate-limit, and internal failures. This includes
`idempotency.reconciliation_required` for an inconsistent command/event
snapshot. Dependency and
rate-limit codes are retryable; the remaining initial codes are not silently
retryable.

`ApplicationError` has a manual `Debug` implementation that excludes message
and detail values from debug output. Details are intended only for reviewed,
safe metadata and are rendered by a later transport contract.

## Verification

Platform unit tests verify stable code/category strings, retryability, display
format, and debug omission of detail values.
