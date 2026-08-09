# ARC-007 — Forward-only migration policy

**Status:** Accepted  
**Decision date:** 2026-08-08  
**Depends on:** [ARC-006 API versioning](ARC-006-api-versioning.md)

## Decision

All PostgreSQL schema changes are checked-in, ordered, forward-only migrations. An applied migration is immutable: its file, identifier, and checksum are never edited or reused. There are no routine down migrations.

Migration files use a unique ordered identifier and descriptive name, for example:

```text
migrations/0001_initial_platform.sql
migrations/0002_match_events.sql
```

The migration runner records the applied identifier and checksum in the database and stops if an already-applied migration has changed. A clean database must be able to apply the complete sequence from the beginning.

## Expand-migrate-contract sequence

Schema changes that affect running code follow this sequence:

1. **Expand** — add new tables, columns, indexes, or nullable constraints without removing data required by the current application.
2. **Compatibility** — deploy code that can safely read/write the old and new representations where a transition requires both.
3. **Backfill** — copy or derive existing data in idempotent, observable batches while preserving source records and recording failures.
4. **Validate** — verify counts, uniqueness, foreign keys, nullability, and domain invariants against representative data.
5. **Contract** — switch all reads/writes to the new representation only after validation evidence exists.
6. **Cleanup** — remove obsolete structures in a later migration after no supported application version depends on them.

An add-and-remove change must not be compressed into one deployment when it would make the previous application binary unable to start or operate safely.

## Data safety rules

- Foreign keys, true uniqueness constraints, check constraints, and not-null requirements are preserved or strengthened through explicit migrations.
- Data transformations never fabricate missing business values. Unknown values remain unknown or are routed to a reviewed staging path.
- Destructive removal requires an explicit scope, backup/recovery evidence, dependency search, and a later forward migration; it is not hidden inside a rename or cleanup script.
- Historical events, audit records, correction evidence, and membership history are not deleted to make a migration easier.
- Large backfills are resumable, bounded, observable, and safe to retry.
- A failed migration leaves an identifiable state and is repaired by a new forward migration or controlled recovery procedure.

## Transaction and operational rules

- Migrations are transactional by default.
- A migration that cannot run in one transaction must document why, its locking/availability impact, and its recovery path before being applied.
- DDL and data changes are not issued ad hoc from application startup or operator commands.
- Production/test migration execution is an explicit deployment/operations action, separate from local schema verification.
- Migration scripts must not depend on speculative services, caches, or multiple authoritative databases.

## Application compatibility

The application and database are deployed with an explicit compatibility window. A previous application binary may remain safe only if the migration follows the expand-migrate-contract sequence. API versioning and database migration numbers are independent; changing one does not silently change the other.

Canonical event schemas are preserved through versioned payloads and explicit upcasting or a controlled forward transformation. An applied event migration must not reinterpret an old event by editing historical meaning in place.

## Rollback and recovery

Rollback means deploying a previously compatible application version or applying a new forward fix; it does not mean running an unreviewed down script. If a schema change has already committed and cannot be safely reversed, recovery uses the approved backup/restore procedure and then resumes through forward migrations.

Every migration increment must include:

- clean-database application verification;
- relevant existing-data verification;
- constraint and repository integration tests;
- a documented failure/recovery behavior when the migration is nontrivial.

## Acceptance criteria

ARC-007 is complete when schema changes have immutable ordered history, safe expand-migrate-contract behavior, preserved data invariants, explicit recovery semantics, and no routine destructive rollback path.

