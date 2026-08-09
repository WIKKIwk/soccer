# QUA-001: provisional and verified match state

**Status:** Implemented
**Decision date:** 2026-08-09
**Depends on:** [P0-001 product contract](P0-001-product-contract.md), [P0-006 initial C1 commands and canonical events](P0-006-c1-commands-events.md), and [RT-015 verification change notifications](RT-015-verification-notifications.md)

## Decision

The match aggregate owns a match-level `MatchVerificationStatus` with two
states:

- `provisional`: the initial state for a match while live observations are
  being collected;
- `verified`: the final-authority state after the future verification workflow
  has applied the official PFL protocol, result, and disciplinary decisions.

This state is deliberately separate from the existing event-level
`EventVerificationStatus`. The latter describes the verification metadata on
one immutable event envelope; it does not establish that the complete match
has been verified.

The model accepts only the two meaningful state changes:
`provisional -> verified` and `verified -> provisional`. Repeating the current
state is rejected with a typed transition error. The status vocabulary and
persisted parser are fail-closed to the stable `provisional|verified` values.

The aggregate starts in `provisional` and keeps the verification transition
independent from football lifecycle transitions. QUA-001 does not yet attach
an actor, supervisor authorization, reason, evidence reference, audit record,
canonical `MatchVerified`/`MatchReopened` producer, or database column for the
match-level state. Those responsibilities belong to QUA-002 through QUA-006
and SRC-001. Existing `match_events.verification_status` remains event-level
metadata and is not repurposed by this increment.

Transport-specific REST and realtime status enums remain separate from the
domain enum. Adapters must receive an explicit match status and must not infer
verification from missing event data.

## Implementation

`football-domain::MatchAggregate` stores the match-level status and exposes a
typed transition method. `football-domain::MatchVerificationStatus` provides
stable serialization vocabulary, parsing, and the transition invariant.

No database migration was added: persistence of the match-level state is
intentionally deferred until the verification workflow and its audit/evidence
requirements are defined.

## Verification

The domain library suite passed with 119 tests and no failures. The final
workspace gates all passed:

- `cargo fmt --all -- --check`;
- `cargo check --workspace --all-targets`;
- `cargo clippy --workspace --all-targets --all-features -- -D warnings`;
- `env -u DATABASE_URL -u TEST_DATABASE_URL -u ALLOW_CLEAN_DATABASE_MIGRATION cargo test --workspace --all-targets --quiet`.

The PostgreSQL integration tests remained ignored because they require an
explicit dedicated `TEST_DATABASE_URL`. The persistent `football_socket_test`
database audit remained at 7 migrations, 1 admin user, 3 commands, and 3
match events; no outbox or realtime identity table exists. No migration was
added by QUA-001.
