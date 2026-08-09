# OPS-005: operator lease release and takeover

Status: implemented

## Decision

The current lease row represents the active ownership state; historical
release and takeover audit records are deferred to the audit increment.

Release uses the current assigned operator and lease ID:

```text
ALLOW_DEVELOPMENT_OPERATIONS_SEED=1 \
DATABASE_URL=<approved-target> \
footballctl match release \
  --match-id <uuidv7> \
  --operator-id <uuidv7> \
  --lease-id <uuidv7>
```

Release locks the assignment and lease in one transaction, verifies operator
ownership, deletes the current lease, and reads the absence back before
returning. Repeating release after the lease is already absent is a safe
idempotent no-op while the assignment still belongs to the caller.

Takeover uses a target operator and a new lease ID:

```text
ALLOW_DEVELOPMENT_OPERATIONS_SEED=1 \
DATABASE_URL=<approved-target> \
footballctl match takeover \
  --match-id <uuidv7> \
  --operator-id <uuidv7> \
  --lease-id <uuidv7> \
  --lease-seconds <positive-integer>
```

Takeover locks the assignment and current lease. An active lease is always
rejected. When the current lease is expired, or when it was explicitly
released and is absent, takeover atomically replaces or creates the lease and
updates the assignment to the target active operator. A lease ID cannot be
reused. The target lease must satisfy the existing future-expiry domain
invariant.

Heartbeat and takeover cannot race into a partial state because both lock the
assignment row before inspecting or changing the lease row. The shared
active/expired boundary is defined by OPS-006. OPS-007 remains responsible for
concurrent two-operator evidence.

## Verification

The ignored integration test
`apps/footballctl/tests/assignment_claim.rs` ran against the disposable
`football_socket_ops005` database and verified active takeover rejection,
release read-back, repeated release no-op, takeover after release, expiry-based
takeover after a one-second lease, and final assignment/lease persistence. The
disposable database was dropped after the test.
