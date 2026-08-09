# CMD-001 — Logical command identity and request fingerprint

**Status:** Accepted  
**Decision date:** 2026-08-08  
**Depends on:** [ARC-003 UUID strategy](ARC-003-uuid-strategy.md), [ARC-008 command, event, and version semantics](ARC-008-command-event-version-semantics.md), [MCH-005 command and event envelopes](MCH-005-command-event-envelopes.md)

## Decision

One logical command has two intentionally distinct identities:

- `CommandId` is the internal canonical UUID used by persistence and event
  causation;
- `ClientCommandId` is the caller-provided UUIDv7 idempotency identity and is
  unique for the lifetime of the command record.

The application combines them with a `RequestFingerprint` into
`LogicalCommandIdentity`. The fingerprint is exactly 32 bytes and represents
SHA-256 of bytes that the command boundary has already canonicalized from the
operation and request payload.

This increment does not invent a JSON canonicalization algorithm. Transport
adapters must produce deterministic canonical bytes before calling
`fingerprint_canonical_bytes`; whitespace, object-key order, and transport
serialization details must not be hashed accidentally. Same canonical bytes
produce the same fingerprint, while a changed canonical operation or payload
produces a different one.

Idempotency comparison, command-row admission, fingerprint conflict handling,
expected-version checks, and terminal result persistence remain later `CMD`
increments. No command or database row is created by these types.

## Verification

Database-free domain tests reject non-UUIDv7 client IDs and preserve the
fixed-size fingerprint. Application tests verify the SHA-256 empty-input
vector, deterministic repetition, changed-input distinction, and separation
of internal and caller command identities.
