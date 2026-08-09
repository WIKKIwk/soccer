# CMD-003 — Short command-admission transaction

**Status:** Implemented and clean-database verified on 2026-08-09  
**Decision date:** 2026-08-08  
**Depends on:** [CMD-001 logical command identity](CMD-001-logical-command-identity.md), [CMD-002 rejection and retry semantics](CMD-002-rejection-retry-semantics.md), [DB-003 migration execution](DB-003-migration-execution.md), [ARC-008 command, event, and version semantics](ARC-008-command-event-version-semantics.md)

## Decision

The first command store uses one `commands` table and a short transaction:

1. insert a new row by `client_command_id` with `RECEIVED`, or do nothing if
   the client identity already exists;
2. load that row with `FOR UPDATE`;
3. compare the fixed-size SHA-256 request fingerprint;
4. return an existing terminal `ACCEPTED`/`REJECTED` row unchanged;
5. reject an unexpired `PROCESSING` lease as `CommandInProgress`;
6. claim `RECEIVED`, `FAILED_RETRYABLE`, or expired `PROCESSING` as
   `PROCESSING` with the new owner and UTC lease expiry;
7. commit only the claim update.

A stored `PROCESSING` row is valid only when both its processing owner and
lease expiry are present. An incomplete lease is rejected as invalid rather
than being interpreted or repaired implicitly.

The domain `CommandStatus` owns the stable persisted vocabulary used by
admission, lease validation, terminal-result loading, and reconciliation.
Infrastructure adapters parse through that single boundary and reject unknown
values without guessing a lifecycle state.

Fingerprint mismatch returns a deterministic conflict and never changes the
command row. The lease expiry is supplied explicitly by the application
boundary; the repository does not call a system clock. This increment stores
only admission metadata. Terminal result payloads, final aggregate version,
aggregate/command type, and event commit belong to later increments.

The migration and SQL adapter are implemented. A focused integration test
applied the full ordered migration set and proved insert, claim, terminal
return, mismatch, active lease, expired-lease recovery, and
`FAILED_RETRYABLE` recovery against a dedicated project test database.

## Verification

Database-free tests verify stored status parsing and reject unknown values;
application tests verify future lease validation. The integration fixture in
`crates/infrastructure/tests/command_admission.rs` ran successfully in two
fresh dedicated databases, one for the full admission state machine and one
for concurrent duplicate admission. No existing database was used as a test
target.
