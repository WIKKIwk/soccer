# OUT-001 — Transactional outbox foundation

**Status:** Schema, versioned envelope, transactional insertion, lease-backed claim, deterministic retry/backoff, poison handling, and publish completion clean-database verified; transport pending  
**Decision date:** 2026-08-09  
**Depends on:** [EVT-005 persisted event metadata](EVT-005-persist-event-metadata.md), [MCH-005 command and event envelopes](MCH-005-command-event-envelopes.md), [APP-001 load-execute-commit orchestration](APP-001-load-execute-commit-orchestration.md)

## Decision

The first outbox slice stores one integration message for each accepted
canonical match event. The message has:

- `message_type = match.event`;
- `schema_version = 1`;
- `message_id = source_event_id`;
- the source match aggregate and aggregate version;
- the causation command and source event IDs;
- a JSON envelope containing the canonical event metadata and typed payload.

The versioned envelope is an integration boundary, not a copy of the command
request. It carries event identity, match identity, aggregate version, event
index/type/schema, period and clock, occurred/recorded timestamps, actor,
verification status, correction links, and the typed event payload. Request
fingerprints and client command IDs are intentionally excluded.

For an accepted C1 command, the executor now orders the caller-independent
transaction as follows:

1. validate the processing lease and append the canonical event;
2. write the command audit row;
3. encode and insert the versioned `match.event` outbox message;
4. persist the accepted terminal command result;
5. commit the transaction.

A rejected command has no canonical source event and therefore no outbox
message. Any failure before commit rolls back the canonical event, audit,
outbox, and terminal-result writes together. The source-event/message unique
constraint prevents a second message for the same event and message type.

The schema now adds a lease-backed delivery state with `pending`,
`processing`, and `published` values, an attempt count, and claim owner/expiry
metadata. The worker claim transaction selects pending or expired-processing
rows in creation order with `FOR UPDATE SKIP LOCKED`, increments the attempt
count, and assigns the new lease atomically. A claim is not a publish
acknowledgement: only the worker that owns an active lease may transition the
row to `published` and record `published_at` through the publish-completion
boundary. The caller commits that acknowledgement in its transaction; a
rollback leaves the processing claim available for expiry recovery. Poison
handling and WebSocket publication remain separate contracts, and no message
is considered published merely because it is present in the outbox table.

## Verification

The clean PostgreSQL `application_c1_execution` fixture applied migrations
1–13 to a dedicated project test database and verified one accepted canonical
event with one matching outbox row, source version `1`, schema version `1`,
the `match.event` type, and a payload without request fingerprint or client
command ID. It also verified that a rejected command creates no outbox row and
that an audit persistence failure rolls back the event and outbox insertion.

The clean PostgreSQL `outbox_claim` fixture applied migrations 1–13 and
verified that two concurrent worker transactions receive disjoint rows while
one transaction holds its locks, an expired processing lease is reclaimed with
an incremented attempt count, and published messages are not claimed again.

The clean PostgreSQL `outbox_retry` fixture applied migrations 1–13 and
verified active-lease ownership, deterministic one- and two-second backoff,
no claim before `next_attempt_at`, claim eligibility once the retry is due,
and terminal poison transition at attempt `10` with no later claim.

The clean PostgreSQL `outbox_publish` fixture applied migrations 1–13 and
verified that a rolled-back source transaction leaves no command, event, or
outbox row; wrong-worker and expired-worker publish acknowledgements are
rejected; a reclaimed worker can complete the publish; and a rolled-back
completion does not persist `published` until the acknowledgement commits.

The workspace test, clippy, and formatting gates pass after this integration.
