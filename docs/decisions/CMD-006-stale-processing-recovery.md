# CMD-006 — Stale-processing recovery

**Status:** Implemented; clean-database recovery and reconciliation guard verified  
**Decision date:** 2026-08-09  
**Depends on:** [CMD-002 final rejection and retryable failure semantics](CMD-002-rejection-retry-semantics.md), [CMD-003 short command admission](CMD-003-short-command-admission.md), [CMD-007 terminal command results](CMD-007-terminal-command-results.md)

## Decision

An expired `PROCESSING` command is not automatically safe to reclaim. The
admission transaction locks the command row and first checks for canonical
`match_events` whose `causation_command_id` is the command being retried.

- If no canonical event evidence exists, the normal admission claim may move
  the command to the new processing owner.
- If canonical event evidence exists, admission returns
  `idempotency.reconciliation_required` and leaves the command row unchanged.
- An active processing lease remains `idempotency.command_in_progress`.

The reconciliation error is a stop signal, not a retry authorization. The
caller must use the read-only command reconciliation boundary and resolve the
uncertain command state before another execution attempt. This first guard
uses canonical event evidence as the conservative minimum; later critical
synchronous state must be included in the same no-blind-retry decision once
that state has its own persistence contract.

## Verification

The clean PostgreSQL fixture
`stale_processing_reclaims_only_without_canonical_event_evidence` verifies
that an expired command with no canonical events is reclaimed, while an
expired command with a causation event returns reconciliation-required and
retains its processing status, owner, and expired lease. Database-free tests
verify the stable application error mapping.
