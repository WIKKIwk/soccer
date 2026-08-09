# CMD-004 — Idempotency fingerprint conflict

**Status:** Stable adapter error mapping and application orchestration verified on 2026-08-09  
**Decision date:** 2026-08-08  
**Depends on:** [CMD-001 logical command identity](CMD-001-logical-command-identity.md), [CMD-003 short command admission](CMD-003-short-command-admission.md), [FND-011 shared application errors](FND-011-shared-application-errors.md)

## Decision

The caller-provided `client_command_id` identifies one logical command for its
full lifetime. When a later request reuses that ID, admission compares its
fixed-size `RequestFingerprint` with the stored fingerprint while holding the
command row lock.

- The same client command ID with the same fingerprint follows the stored
  command status path.
- The same client command ID with a different fingerprint returns an
  idempotency conflict and does not claim, retry, or mutate the stored row.
- The conflict is represented internally as
  `CommandAdmissionError::FingerprintMismatch`.
- The application/transport boundary maps it to the stable non-retryable
  `idempotency.fingerprint_mismatch` error code. The stored internal command
  ID may be used as safe correlation metadata; the original or received
  fingerprint bytes are never returned or logged as public error details.

Fingerprint mismatch is final for that request. It is not a transient lease
conflict and must not be retried automatically with the same mismatched
payload.

## Verification boundary

The admission adapter and clean-database fixture contain the row-lock
comparison and mismatch branch. `CommandAdmissionError::to_application_error`
maps the adapter result to the stable platform taxonomy and retains only a safe
internal command ID as correlation metadata. The application fixture
`crates/infrastructure/tests/application_admission.rs` runs the concrete
`CommandOrchestrator` with `PostgresCommandAdmission`, proves that a mismatch
stops before execution, and verifies the stable non-retryable application code.

## Verification

Database-free infrastructure tests verify that the fingerprint conflict and
active command-lease conflict map to idempotency errors, are non-retryable, and
do not expose fingerprint or persisted-state details. The clean-DB admission
fixture and the application-orchestrator fixture both passed. The stored
command row remains unchanged and no second command row is created.
