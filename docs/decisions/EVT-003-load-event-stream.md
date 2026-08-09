# EVT-003 — Load a canonical event stream

**Status:** Implemented as a read-only boundary and clean-database verified on 2026-08-09  
**Decision date:** 2026-08-08  
**Depends on:** [EVT-001 canonical match-event migration](EVT-001-canonical-match-event-migration.md), [EVT-002 event schema versioning](EVT-002-event-schema-versioning.md), and [ARC-008 command/event version semantics](ARC-008-command-event-version-semantics.md)

## Decision

The generic infrastructure adapter loads one match's rows with an explicit
`ORDER BY aggregate_version ASC`, maps persisted typed columns into the domain
event envelope, and preserves the raw JSON payload as `serde_json::Value`.
Both pool-level and caller-transaction variants use the same row mapping. The
C1-specific adapters apply the EVT-006 codec after this generic load and
return typed payloads for domain reduction. The transaction variants preserve
the caller's database snapshot for a later load/append/terminal-result
operation.

The domain `CanonicalEventStream` then verifies that every row belongs to the
requested match, aggregate versions are contiguous from version `1`, and each
causation command's event indexes advance from zero without gaps. An empty
stream represents the initial aggregate version `0`. A mixed match, gap,
duplicate, or out-of-order result is rejected; the loader never sorts or
silently repairs canonical history.

Period, verification status, event type, schema version, numeric football time,
and timestamps are parsed explicitly. Unknown persisted vocabulary is returned
as an error rather than guessed. The domain owns the persisted strings for
`MatchPeriod` and `EventVerificationStatus`; infrastructure uses those same
values for both read and write paths.

## Verification

Database-free domain tests cover empty/contiguous streams, mixed-match streams,
duplicate event IDs, command-local event-index gaps, and version gaps.
Infrastructure tests cover valid row mapping and invalid period, verification,
aggregate-version, and clock values. The ignored PostgreSQL round-trip now
also reads the typed C1 stream before committing the same transaction, while
the SQL query and row mapping compile under the workspace checks.

The typed PostgreSQL round-trip passed in a fresh dedicated database,
including same-transaction and post-commit stream loading plus command-local
event lookup. No existing database was modified.
