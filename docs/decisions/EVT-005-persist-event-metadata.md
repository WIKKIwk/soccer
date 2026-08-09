# EVT-005 — Persist causation command ID and event index

**Status:** Implemented as a transaction adapter and clean-database verified on 2026-08-09  
**Decision date:** 2026-08-09  
**Depends on:** [EVT-004 append multiple events](EVT-004-append-multiple-events.md), [EVT-001 canonical match-event migration](EVT-001-canonical-match-event-migration.md), and [ARC-008 command/event version semantics](ARC-008-command-event-version-semantics.md)

## Decision

`persist_match_event_batch` accepts the loaded aggregate's explicit current
version and already-versioned domain envelopes, then persists the complete
canonical metadata in the caller-owned PostgreSQL transaction. It binds
`causation_command_id` and `event_index` directly from each envelope; it never
derives either value from UUID order or timestamps.

The batch is encoded as one `INSERT ... VALUES` statement. Empty batches are a
no-op. Before the insert, the adapter validates that the supplied already-
versioned batch stays on one match, advances aggregate versions contiguously
after the supplied current version, preserves zero-based command-local indexes,
and contains no duplicate event IDs within the batch. The C1 execution adapter
loads that current version in the same transaction before appending. A
non-empty batch then fails as one statement if any row violates a database
constraint.
The surrounding command transaction remains responsible for lease validation,
critical state, audit, outbox, terminal result, and the final commit.

The adapter explicitly maps domain period and verification enums to the
persisted vocabulary, preserves JSON payloads, and rejects u64 values that do
not fit PostgreSQL `BIGINT` rather than truncating them. The typed
`persist_c1_event_batch` wrapper first applies the EVT-006 payload codec and
then delegates to the same caller-owned transaction.

## Verification

Database-free infrastructure tests verify exact causation-command and
zero-based event-index encoding, payload preservation, numeric overflow
rejection, and typed C1 payload conversion. The clean PostgreSQL C1 and
typed-event fixtures also read back causation command IDs and zero-based event
indexes after commit. Workspace tests and clippy pass.
