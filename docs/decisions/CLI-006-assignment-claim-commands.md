# CLI-006: match assignment and operator claim commands

Status: implemented

## Decision

The first operations slice stores the explicit match-to-fixture-to-operator
assignment and one operator lease for that match. The command surface is
development-authorized and requires UUIDv7 identities supplied by the caller:

```text
ALLOW_DEVELOPMENT_OPERATIONS_SEED=1 \
DATABASE_URL=<approved-target> \
footballctl match assign \
  --match-id <uuidv7> \
  --fixture-id <uuidv7> \
  --operator-id <uuidv7>

ALLOW_DEVELOPMENT_OPERATIONS_SEED=1 \
DATABASE_URL=<approved-target> \
footballctl match claim \
  --match-id <uuidv7> \
  --operator-id <uuidv7> \
  --lease-id <uuidv7> \
  --lease-seconds <positive-integer>
```

An assignment requires an existing fixture and an active administrator record
for the operator. A match assignment is idempotent only when all three
identities match. A fixture cannot be assigned to a second match.

A claim requires an existing assignment and the assigned active operator. One
match has at most one lease. Repeating a claim with the same match, operator,
and active lease ID returns the existing lease without extending it. A
different lease ID is rejected while the existing lease is active. The lease
domain invariant requires `expires_at > claimed_at`.

The database implementation is transactional. `0017_match_assignments_leases.sql`
adds `match_assignments` and `match_operator_leases`; foreign keys preserve
fixture and administrator ownership, while unique constraints preserve the
one-fixture/one-match and one-lease/match boundaries.

This increment does not introduce heartbeat, release, takeover, expiry worker,
RBAC, supervisor assignment, or public API endpoints. Those remain separate
operations/identity/quality increments.

## Verification

The ignored integration test
`apps/footballctl/tests/assignment_claim.rs` ran against the disposable
`football_socket_cli006` database and verified migration application,
idempotent assignment and claim, fixture reuse conflict, active lease conflict,
wrong-operator rejection, and persisted read-back. The disposable database was
dropped after the test.
