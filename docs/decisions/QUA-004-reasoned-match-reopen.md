# QUA-004: reasoned match reopen

**Status:** Implemented; reason validation, canonical reopen append, typed persistence, and clean-database reload verified
**Decision date:** 2026-08-09
**Depends on:** [P0-006 initial C1 commands and canonical events](P0-006-c1-commands-events.md), [QUA-001 provisional and verified match state](QUA-001-provisional-verified-state.md), [QUA-002 supervisor verification permissions](QUA-002-supervisor-verification-permissions.md), and [QUA-003 match verification event](QUA-003-match-verification.md)

## Decision

`ReopenMatch` is a supervisor-only, reasoned governance command. It appends
exactly one immutable `MatchReopened` event and never deletes, edits, or
replaces the earlier `MatchVerified` event or any C1 event.

The command is accepted only when the latest verification marker in the
canonical stream is `MatchVerified`. A missing verification marker or a
stream already reopened without a subsequent verification returns a typed
`MatchNotVerified` transition error. The expected aggregate version is
checked before the state check and append. After a successful reopen, a new
`VerifyMatch` is allowed at the new version, preserving the alternating
`verified -> provisional -> verified` history.

The event contract is frozen as follows:

- event type: `MatchReopened`;
- schema version: `1`;
- event-level `verification_status`: `provisional`;
- payload: exactly `{ "reason": "..." }`;
- reason: non-blank `CorrectionReason`, preserved without trimming or
  truncation;
- correction references: absent.

## Authorization and transaction boundary

The QUA-002 application policy remains the authoritative supervisor-only
authorization boundary for `reopen_match`; this increment does not infer
permission from an administrator UUID or UI visibility. Authentication,
persistent role assignment, and capability loading remain the IDN increments.

`execute_reopen_match_in_transaction` validates command identity, actor, and
expected version; proves the processing lease inside the caller-owned
transaction; loads the typed canonical union; appends the domain reopen
command; and persists only the new event batch. Audit, outbox, terminal
result, and final commit remain owned by the surrounding application
executor, consistent with QUA-003 and the current narrow command adapters.

## Persistence impact

No migration was added. The existing `match_events` envelope already stores
the event type, schema version, provisional status, JSONB payload, actor, and
nullable correction-reference fields. The codec validates the exact reason
shape, rejects unknown fields and blank reasons, rejects correction
references, and rejects a verification event whose resulting status does not
match its event type.

## Verification

- Domain tests cover the stable vocabulary, provisional reopen metadata,
  verified-state precondition, repeated reopen rejection, and re-verification
  after reopen: 124 passed, 0 failed.
- Infrastructure tests cover `MatchReopened` JSONB round-trip, raw-row typed
  union decoding, status/reason/reference validation, adapter context/error
  mapping, and the transaction boundary: 92 passed, 0 failed, 1
  database-dependent test ignored.
- Clean `football_socket_qua004` integration fixture passed migration
  preflight, verify-then-reopen command admission and lease checks, two
  commits, typed stream reload, and exact JSONB reason readback.
- Final workspace gates passed: `cargo fmt --all -- --check`,
  `cargo check --workspace --all-targets`,
  `cargo clippy --workspace --all-targets --all-features -- -D warnings`, and
  the DB-URL-disabled full workspace test command.
- The disposable fixture database was dropped after verification. The
  persistent `football_socket_test` database was not modified.
