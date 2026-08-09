# QUA-003: match verification event

**Status:** Implemented; canonical verification append, codec, projection, and clean-database persistence verified
**Decision date:** 2026-08-09
**Depends on:** [P0-001 product contract](P0-001-product-contract.md), [P0-006 initial C1 commands and canonical events](P0-006-c1-commands-events.md), [QUA-001 provisional and verified match state](QUA-001-provisional-verified-state.md), and [QUA-002 supervisor verification permissions](QUA-002-supervisor-verification-permissions.md)

## Decision

`VerifyMatch` appends exactly one immutable `MatchVerified` event to the
canonical match-event union. The command carries the normal command/event
identity, expected aggregate version, football timing metadata, UTC
timestamps, and authenticated actor identity. The append is optimistic:
the supplied expected version must equal the loaded canonical version.

The command rejects a second `MatchVerified` marker while the stream contains
the first one. Reopening is a separate future `ReopenMatch` decision and is
not inferred by overwriting or deleting the existing verification event.

`MatchVerified` has schema version `1`, event-level
`verification_status = verified`, an empty JSON object payload (`{}`), and no
correction references. The supervisor identity remains in the canonical
envelope actor field. Final-authority evidence and source references are not
fabricated by this increment; they belong to the evidence and audit
increments.

Verification markers are governance events, not C1 football actions. The
effective-stream match projection ignores them for score and timeline while
retaining the canonical aggregate version as its synchronization point.

## Authorization and transaction boundary

The QUA-002 application policy remains the authoritative supervisor-only
authorization boundary. A caller must authorize `VerifyMatch` with that
policy before constructing/admitting this command; the event-store adapter
does not derive permission from an administrator UUID or from UI visibility.
Authentication, persistent role assignment, and capability loading remain
the IDN increments.

`execute_verify_match_in_transaction` validates the command execution
context, proves the processing lease inside the caller-owned PostgreSQL
transaction, loads the typed canonical union, appends the domain command,
and persists only the new event batch. It returns an uncommitted stream so a
later application executor can compose audit, outbox, terminal-result, and
commit ordering. This increment therefore does not claim those later
side-effects as complete.

## Persistence impact

No migration was added. The existing `match_events` envelope already accepts
the stable `MatchVerified` event type, schema version, verified status, empty
JSONB object, actor foreign key, and nullable correction references. The
typed repository decoder now recognizes the verification member of the
canonical union and fails closed for unknown payload fields or correction
references.

## Verification

- Domain unit suite: 123 passed, 0 failed.
- Infrastructure unit suite: 87 passed, 0 failed, 1 pre-existing
  database-dependent test ignored.
- Clean `football_socket_qua003` integration fixture: migration preflight,
  supervisor seed, command admission/lease validation, transactional append,
  commit, typed reload, and JSONB payload readback passed.
- Final workspace gates passed: `cargo fmt --all -- --check`,
  `cargo check --workspace --all-targets`,
  `cargo clippy --workspace --all-targets --all-features -- -D warnings`, and
  the DB-URL-disabled full workspace test command.
- The disposable fixture database was dropped after verification. The
  persistent `football_socket_test` database was not modified.
