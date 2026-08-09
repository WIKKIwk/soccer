# QUA-005 — Correction and verification decision audit

**Status:** Implemented; typed decision details, PostgreSQL persistence, commit, reload, and rollback verified
**Decision date:** 2026-08-09
**Depends on:** [APP-002 expected version, idempotency, and audit boundary](APP-002-expected-version-idempotency-audit.md), [COR-001 correction reason and immutable void event](COR-001-correction-reason-and-void-event.md), [QUA-003 match verification](QUA-003-match-verification.md), [QUA-004 reasoned match reopen](QUA-004-reasoned-match-reopen.md), and [SRC-001 safe evidence reference](SRC-001-safe-evidence-reference.md)

## Decision

Critical correction and verification decisions use the existing command-audit
row rather than a second audit table. `CriticalDecisionAudit` provides the
stable action and safe JSON details for:

- `VoidEvent` — decision name, target immutable event ID, reason, and optional
  evidence reference;
- `VerifyMatch` — decision name and optional evidence reference;
- `ReopenMatch` — decision name, reason, and optional evidence reference.

`CommandAuditEntry::accepted_decision` records one canonical event, the
aggregate version, authenticated actor, decision action, and typed details in
one audit entry. Evidence is optional in this increment because source type,
reliability, and mandatory critical-correction linking are separate `SRC-002`
and `SRC-003` decisions. When present, the opaque `EvidenceReference` is
preserved as supplied and is never treated as a fetch instruction.

Rejected commands continue to use the existing `CommandAuditEntry::rejected`
shape with a stable rejection code and safe object details. Neither accepted
nor rejected audit entries copy request payloads or idempotency fingerprints.

## Transaction boundary

The narrow correction, verification, and reopen adapters still return an
uncommitted canonical stream to their surrounding executor. That executor
constructs the typed decision audit entry and calls `persist_command_audit`
before committing the canonical event and the rest of the command result in
the same PostgreSQL transaction. No second canonical event or audit authority
is introduced.

## Implementation

`football-infrastructure::CriticalDecisionAudit` and
`CommandAuditEntry::accepted_decision` live in
`crates/infrastructure/src/audit.rs` and are exported from the infrastructure
crate. The existing `command_audit` migration is sufficient; no migration was
added.

## Verification

- Infrastructure unit suite: 95 passed, 0 failed, 1 existing database-dependent
  test ignored.
- Clean `football_socket_qua005` integration fixture passed: migration
  preflight, three decision audit rows for correction/verify/reopen, exact
  action/details/event readback, and transaction rollback with no leaked audit
  row.
- The disposable fixture database was dropped after the run. The persistent
  `football_socket_test` database was not modified.
