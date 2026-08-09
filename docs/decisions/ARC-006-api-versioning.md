# ARC-006 — API versioning rules

**Status:** Accepted  
**Decision date:** 2026-08-08  
**Depends on:** [ARC-005 error taxonomy](ARC-005-error-taxonomy.md)

## URL version boundaries

The initial HTTP contracts use explicit major versions in the path:

```text
public API:  /v1/...
admin API:   /admin/v1/...
```

The path version is authoritative. A header-only version negotiation scheme is not used. `footballctl` calls the admin API contract; it does not create a second business API.

## Compatibility rules within a major version

Within `v1`:

- existing fields, meanings, status codes, error codes, and authorization requirements remain compatible;
- optional response fields may be added;
- new optional request fields may be added when their absence preserves existing behavior;
- pagination, cursor, filter, and sorting semantics cannot silently change;
- existing field types, requiredness, and meanings cannot change;
- an existing enum value cannot be repurposed or removed;
- clients must tolerate unknown response fields and safely handle unknown future enum values;
- persistence rows are never exposed as transport contracts.

Changing a field type, removing or requiring an existing field, changing the meaning/default of a field, changing an error contract, or tightening authorization in a way that breaks an existing client is a breaking change.

## New major versions

A breaking contract creates a new major path, for example `/v2/...` or `/admin/v2/...`. The old major remains available during an explicit migration period; it is not silently rewritten to the new semantics. Deprecation requires documentation, client impact identification, migration guidance, and an evidence-based removal decision.

API major versioning is independent from application release numbers, database migration numbers, and domain event schema versions.

## Contract ownership

- Versioned request/response DTOs live in `crates/contracts`.
- Public and admin handlers map DTOs to application commands/queries; DTOs are not domain entities.
- OpenAPI is updated in the same increment as the implemented endpoint and checked by contract tests.
- Stable machine-readable error codes follow ARC-005 and are versioned with the API contract.
- Every response includes the roadmap metadata contract where applicable:

```json
{
  "data": {},
  "meta": {
    "version": 1,
    "generated_at": "UTC timestamp",
    "verification_status": "provisional|verified",
    "quality_flags": []
  }
}
```

`meta.version` identifies the response contract version, not the match aggregate version. Aggregate versions are returned separately where clients need concurrency or synchronization information.

## Command and idempotency compatibility

Every mutation endpoint keeps the command identity, request fingerprint, and expected-version semantics defined by the domain contract. Retrying the same client command ID with the same fingerprint is safe; reusing it with a different fingerprint remains an idempotency conflict across the lifetime of that API major.

Admin and public API versions do not bypass domain authorization, audit, correction, or transaction rules. A new API adapter must call the same application command/query ports.

## Realtime and schema versions

WebSocket is versioned independently from REST transport:

- topic names remain stable within a realtime protocol major;
- notifications and patches carry a versioned event type/schema;
- canonical snapshots are fetched through the versioned REST API;
- a client receiving an unsupported event schema must request a snapshot or use the documented fallback, never guess the patch meaning.

Canonical match event schema versions are distinct from HTTP API versions. Event replay must continue to understand persisted historical schemas through explicit upcasters or a controlled migration, not by changing the meaning of an old event in place.

## Acceptance criteria

ARC-006 is complete when public and admin paths have explicit major versions, compatible versus breaking changes are defined, contracts/OpenAPI have one owner, idempotency semantics survive retries, and REST, WebSocket, and canonical event versions cannot be confused.

