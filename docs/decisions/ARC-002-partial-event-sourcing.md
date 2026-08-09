# ARC-002 — Partial event sourcing for match behavior

**Status:** Accepted  
**Decision date:** 2026-08-08  
**Depends on:** [ARC-001 modular-monolith boundary](ARC-001-modular-monolith-boundary.md)

## Decision

The platform uses partial event sourcing. Only behavior whose correctness depends on an ordered, replayable match history is stored as an append-only canonical event stream. The entire database is not event-sourced.

## Event-sourced match behavior

The canonical event stream is authoritative for:

- match lifecycle transitions;
- match clock transitions and corrections;
- pre-match lineup submission and confirmation;
- C1 live match actions;
- event corrections and voids;
- critical verification and reopen decisions.

Each accepted event belongs to one match aggregate, has a causation command, an event schema version, an aggregate version, and an immutable payload. Events are appended in deterministic order. A published event is never updated or deleted in place.

## Relational CRUD behavior

Ordinary master data remains relational state with explicit constraints, audit, and history where required:

- administrators, roles, sessions, and permissions;
- countries, regions, cities, and venues;
- clubs, teams, persons, player profiles, and localized names;
- competitions, seasons, stages, groups, rounds, and rules;
- squad membership, competition registration, and scheduling metadata.

Relational records do not become fake domain events merely because an audit row exists. History is preserved where the business meaning requires it, especially for membership, rules, corrections, and archive decisions.

## Reduction and replay

The match aggregate is rebuilt by loading its canonical event stream and reducing events in aggregate-version order. The reducer must be deterministic: the same valid stream produces the same lifecycle, score, clock, lineup, player-on-pitch state, timeline, and correction result.

Synchronous state maintained in the command transaction is a rebuildable optimization for current operational reads:

- current match status;
- current score;
- current clock state;
- players on the pitch;
- current aggregate version;
- minimal timeline head.

This state never overrides the event stream. If it disagrees with replay, replay is authoritative and the inconsistency is surfaced and repaired through a controlled rebuild.

## Corrections and voids

Corrections do not mutate the original event. A correction or void appends a new canonical event that references the affected event and includes a reason. The effective event-stream builder resolves the correction chain and the reducer rebuilds derived match state.

The builder must reject cyclic correction chains. Original evidence remains queryable for audit and review.

## Projection boundary

The match stream and critical synchronous state commit atomically with audit and outbox messages. Derived read models may be rebuilt from the canonical stream.

Asynchronous projections include:

- standings;
- leaderboards and competition form;
- aggregated statistics;
- quality aggregates;
- notifications and search documents;
- future analytics.

The outbox is not authoritative and no integration event is published before the domain transaction commits.

## Snapshot rule

Snapshots are optional replay optimizations. They may be introduced only after replay performance is measured, must carry the aggregate version they represent, and must be validated against the event stream. A snapshot can be discarded and rebuilt; it is never authoritative state.

## Explicit non-goals

- event-sourcing every master-data table;
- treating logs, WebSocket messages, cache, or external search documents as canonical state;
- allowing an operator-provided total score to overwrite the reduction;
- deleting or editing published events;
- adding a broker or separate event store before measured operational need.

## Acceptance criteria

ARC-002 is complete when match behavior has one append-only source of truth, a deterministic reducer/replay path, immutable correction semantics, and a clear relational CRUD boundary for non-match master data.

