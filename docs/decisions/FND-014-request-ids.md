# FND-014 — Request IDs

**Status:** Accepted  
**Decision date:** 2026-08-08  
**Depends on:** [ARC-003 UUID strategy](ARC-003-uuid-strategy.md), [FND-013 structured logging](FND-013-structured-logging.md)

## Decision

The HTTP platform middleware generates one UUIDv7 `RequestId` for every
incoming request. The server does not trust an inbound `x-request-id` value;
any inbound value is ignored and the generated ID is used for the request.

The generated ID is:

- inserted into request extensions for handlers and future error boundaries;
- returned in the response `x-request-id` header;
- included in the request span and completion log fields.

The canonical serialized form is the lowercase, hyphenated UUID representation
defined by the UUID library. A request ID is a correlation identifier only; it
is not an authentication secret, authorization proof, or idempotency key.

## Boundary

Request ID generation belongs to `football-platform::http`, so all first-party
HTTP adapters can apply the same behavior. The admin API applies the middleware
at the router boundary. Future error mapping can read the `RequestId`
extension and populate `ApiErrorMeta.request_id` without generating a second
identifier.

## Verification

Platform tests verify that the middleware generates a UUIDv7, replaces a
client-supplied header, exposes the same value through request extensions and
the response header, and preserves a successful response. The admin API smoke
test verifies that the live route returns `x-request-id` and emits the request
completion record as structured JSON.
