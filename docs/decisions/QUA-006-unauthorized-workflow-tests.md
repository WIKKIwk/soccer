# QUA-006 — Unauthorized verification-workflow tests

**Status:** Implemented; pre-mutation denial and operation-specific capability tests verified
**Decision date:** 2026-08-09
**Depends on:** [QUA-002 supervisor verification permissions](QUA-002-supervisor-verification-permissions.md), [QUA-003 match verification](QUA-003-match-verification.md), and [QUA-004 reasoned match reopen](QUA-004-reasoned-match-reopen.md)

## Decision

The application authorization policy is the mandatory gate before a
`VerifyMatch` or `ReopenMatch` mutation is constructed, admitted, or executed.
QUA-006 adds workflow-level tests around that ordering:

- a `MatchDataOperator` is denied for both operations even when both
  capability facts are incorrectly true;
- a denied operator workflow never reaches the mutation callback;
- a supervisor with `can_verify = false` is denied for verification while a
  separately authorized reopen workflow reaches mutation;
- capability facts remain operation-specific and fail closed.

The test workflow uses a side-effect counter as the mutation boundary. A
counter increment is the only observable mutation, so the test proves the
authorization error happens before the command-side effect without inventing
a transport, session, role-assignment, or database executor that is not yet in
the repository.

## Scope boundary

No database migration or canonical event change is needed. The pure
application policy remains independent from PostgreSQL and from the narrow
infrastructure event executors. Authentication, persistent RBAC, assignment
scope, and HTTP/CLI wiring remain the later identity and adapter increments;
those callers must invoke the same policy before mutation.

## Implementation

`crates/application/src/verification_authorization.rs` now contains the
pre-mutation workflow tests alongside the existing policy tests. The production
authorization API and its fail-closed role/capability semantics are unchanged.

## Verification

- `cargo test -p football-application --lib` passed: 48 tests, 0 failures.
- `cargo fmt --all -- --check` passed after formatting.
- No database was used or changed by this increment.
