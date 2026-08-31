# PUB-001 — Public canonical match REST

**Status:** Implemented; canonical detail/timeline routes and transport checks passed
**Decision date:** 2026-08-31
**Depends on:** [ARC-006 API versioning](ARC-006-api-versioning.md), [ARC-009 projection boundaries](ARC-009-projection-boundaries.md), and [RT-012 canonical REST snapshot](RT-012-canonical-rest-snapshot.md)

## Decision

The public process exposes two read-only version-1 resources:

- `GET /v1/matches/{match_id}` returns `MatchSnapshotResponse`;
- `GET /v1/matches/{match_id}/timeline` returns the same canonical timeline in
  the narrower `MatchTimelineResponse` envelope.

The query adapter resolves the match assignment and fixture, loads the typed
canonical event stream, applies correction/void semantics through the
effective stream, and reduces score and timeline from that source. It supplies
verification state explicitly from the latest effective verification marker.
No SQL row is serialized as a public contract and no missing match data is
invented.

The binary owns a separate `PUBLIC_API_BIND_ADDR` setting, defaulting to
`127.0.0.1:3001`, while reusing the shared database, logging, request-ID, and
health boundaries. Invalid non-UUIDv7 identities fail before database access.

## Contract and verification

`openapi/public-v1.yaml` freezes both paths, their success envelopes, and the
stable public error shape. Contract tests check the timeline derivation and
the required OpenAPI paths/schema names. Router tests verify liveness without
a database and fail-fast validation for malformed match IDs. Domain/query
tests verify an empty canonical stream is represented explicitly as a
provisional zero-score snapshot rather than fabricated state.
