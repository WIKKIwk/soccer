# FND-012 — Application error to HTTP mapping

**Status:** Accepted  
**Decision date:** 2026-08-08  
**Depends on:** [FND-011 shared application errors](FND-011-shared-application-errors.md), [ARC-005 error taxonomy](ARC-005-error-taxonomy.md)

## Decision

HTTP mapping lives at the `football-platform::http` adapter boundary. It does
not add HTTP status codes to `ApplicationError` itself.

The mapping follows ARC-005:

- request → `400`;
- authentication → `401`;
- authorization → `403`;
- not found → `404`;
- validation and domain → `422`;
- conflict and idempotency → `409`;
- rate limit → `429`;
- dependency → `503`; and
- internal → `500`.

`HttpApplicationError` implements Axum `IntoResponse`. The response uses the
versioned contracts error envelope with stable code/category strings, safe
message, structured metadata details, retryability, and optional request or
client-command IDs. It does not expose Rust error variants or database details.

## Verification

Platform tests cover every ARC-005 category mapping and verify that an
application error becomes the expected transport body without changing its
stable code, message, details, retryability, or request metadata.
