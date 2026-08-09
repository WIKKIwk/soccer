# OPS-007: two-operator lease concurrency

Status: implemented

## Decision

Two operators may attempt takeover of the same match at the same time only
after the current lease has expired. The repository serializes that decision
by locking the match assignment row before reading or replacing the current
lease:

1. the first transaction obtains the assignment lock, replaces the expired
   lease, updates the assignment, and commits;
2. the waiting transaction then reads the committed replacement under the same
   lock, observes that it is active at its requested `claimed_at`, and returns
   the typed `ActiveLeaseConflict` error;
3. the losing transaction persists no assignment or lease mutation.

The winner is intentionally nondeterministic. The invariant is exactly one
winner, one conflict, one current lease row, and an assignment whose operator
matches that lease. A losing lease ID is never inserted, and the expired lease
identity is removed by the winner’s replacement.

The concurrency evidence lives in
`crates/infrastructure/tests/operator_concurrency.rs`. It uses two active
operators, an expired initial lease, two distinct takeover lease IDs, and
`tokio::join!` against one PostgreSQL pool. The test asserts the concrete
conflict metadata, persisted assignment/lease identity, single-row invariant,
and absence of both the losing and expired lease IDs.

## Verification

The ignored integration test
`concurrent_two_operator_takeovers_allow_only_one_winner` ran against the
disposable `football_socket_ops007` database and passed. No migration was
added. The disposable database is dropped after verification; the persistent
`football_socket_test` database remains outside the test target.
