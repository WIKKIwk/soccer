# ARC-005 — Error taxonomy

**Status:** Accepted  
**Decision date:** 2026-08-08  
**Depends on:** [ARC-002 partial event sourcing](ARC-002-partial-event-sourcing.md)

Errors are typed at the domain/application boundary, mapped to stable machine-readable codes by the platform boundary, and rendered as transport-specific responses by the adapter. HTTP handlers and CLI commands do not invent football rules or classify errors independently.

## Error categories

| Category | Meaning | Retry behavior |
|---|---|---|
| `request` | Malformed syntax, missing required field, unsupported parameter, or body-limit violation | Final until the request is changed. |
| `authentication` | Missing, invalid, expired, or revoked credentials | Final for the request; obtain a valid session before retrying. |
| `authorization` | Authenticated actor lacks the required resource/action permission | Final unless access scope changes. |
| `not_found` | Referenced resource, aggregate, command, or event does not exist in the permitted scope | Final. |
| `validation` | Input value or cross-field shape is invalid | Final until the input is changed. |
| `domain` | Football invariant or lifecycle rule rejects an otherwise well-formed command | Final; the command ID is consumed and the rejection is replayed identically. |
| `conflict` | Expected version, operator lease, uniqueness, or concurrent ownership conflict | Usually final for the current version; reload and issue a new command when appropriate. |
| `idempotency` | Same command ID was reused with a different request fingerprint, or command state requires reconciliation | Fingerprint mismatch is final; uncertain commit is reconciled by command ID. |
| `rate_limit` | Caller exceeded the configured request or mutation limit | Retry after the server-provided delay when present. |
| `dependency` | Required database or infrastructure dependency is unavailable or timed out | Retryable only when no canonical commit occurred. |
| `internal` | Unexpected software failure not safely attributable to the caller | May be retryable only after command-state reconciliation. |

## Stable code format

Codes are lowercase, dot-separated, and stable across message localization:

```text
<category>.<specific_reason>
```

Initial examples:

```text
request.invalid_json
request.body_too_large
authentication.required
authentication.invalid_session
authorization.forbidden
resource.not_found
validation.required_field
validation.invalid_value
domain.invalid_transition
domain.lineup_duplicate_player
domain.player_not_on_pitch
domain.match_interrupted
domain.correction_cycle
conflict.expected_version
conflict.operator_lease
conflict.unique_constraint
idempotency.fingerprint_mismatch
idempotency.command_in_progress
idempotency.reconciliation_required
dependency.database_unavailable
dependency.timeout
rate_limit.exceeded
internal.unexpected
```

Codes are persisted where the command result is persisted. A final command rejection stores its stable rejection code and safe structured details; retrying the same command ID returns the same result.

## Domain and command outcomes

The application distinguishes these outcomes:

1. **Accepted** — canonical events, critical synchronous state, audit, outbox, and final command result commit atomically.
2. **Final domain rejection** — no canonical match events are written; the command becomes `REJECTED` with a stable code and audit record.
3. **Retryable infrastructure failure** — the command becomes retryable only when no canonical commit occurred.
4. **Uncertain commit** — the application queries command/event state by command ID before attempting execution again; it never blindly duplicates a possibly committed command. An inconsistent snapshot maps to `idempotency.reconciliation_required` and is not automatically retried.

An expected-version conflict, idempotency fingerprint mismatch, and lease conflict are not silently retried. They return the conflict code and the metadata needed for the caller to recover safely.

## HTTP mapping

The initial admin/public adapters use this mapping:

| Error condition | HTTP status |
|---|---:|
| malformed request or unsupported input | `400` |
| missing/invalid authentication | `401` |
| insufficient permission | `403` |
| resource not found | `404` |
| expected-version, lease, uniqueness, or idempotency conflict | `409` |
| well-formed request rejected by a football/domain invariant | `422` |
| rate limit exceeded | `429` |
| retryable dependency failure | `503` |
| unexpected internal failure | `500` |

The mapping is an adapter concern. Domain code returns typed errors and does not know HTTP status codes.

## Error response envelope

Every API error has a stable shape:

```json
{
  "error": {
    "code": "conflict.expected_version",
    "category": "conflict",
    "message": "The match changed before this command was applied.",
    "details": {},
    "retryable": false
  },
  "meta": {
    "request_id": "uuid",
    "client_command_id": "uuid"
  }
}
```

`code` and structured `details` are for machines. `message` is safe presentation text and may be localized at the boundary; clients must not branch on message text. Sensitive values, credentials, full rejected payloads, and database internals are never exposed.

## Observability

Structured logs and metrics include request ID, client command ID when applicable, actor ID, match ID when applicable, error code, latency, and retryability. Stack traces and dependency details remain internal. Redaction applies before logging or persisting command-attempt metadata.

## Acceptance criteria

ARC-005 is complete when every application outcome has a stable category/code, final versus retryable semantics are explicit, HTTP mapping is centralized, persisted command rejections are replayable, and error responses cannot require clients to parse human messages.
