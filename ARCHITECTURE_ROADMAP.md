# Football Data Platform — Architecture and One-Year Implementation Roadmap

> Status: living source of truth  
> Project root: `socket/`  
> Initial repository state: empty  
> Delivery model: one developer, one verified increment at a time  
> Last revised: 2026-08-08

## 1. Purpose

This document is the single implementation roadmap for building an evolvable football data platform over one year.

The system must not be implemented as one large module or one long feature branch. Every work session should deliver one small, complete, testable increment:

1. select one unchecked task;
2. confirm its module boundary and acceptance criteria;
3. inspect the current implementation;
4. write or update the relevant test;
5. implement the smallest production-quality change;
6. run focused verification;
7. update this roadmap only when evidence exists;
8. start the next task only after the current gate passes.

Anything described as `target`, `planned`, or `future` does not exist yet. It must be created only in its scheduled increment.

This roadmap is intentionally detailed, but the schedule is an ordering guide rather than a deadline guarantee. If a gate fails, later work does not start until the failure is resolved.

---

## 2. Product vision

The platform will produce, validate, preserve, and distribute trustworthy football data, initially for one competition and one season.

The core operating flow is:

```text
Administrator configures competition data
                    ↓
Operator submits a match command
                    ↓
Application layer authenticates and coordinates
                    ↓
Domain engine validates football invariants
                    ↓
Canonical event and critical state commit atomically
                    ↓
Read projections expose current public state
                    ↓
Outbox distributes asynchronous and real-time updates
```

The primary product is not the mobile interface. It is the operational data system that prevents invalid state, preserves correction history, makes operator actions auditable, and can rebuild match state deterministically.

---

## 3. First-year product boundary

### 3.1 Core MVP

The first-year core deliverable includes:

- one competition and one season;
- clubs, teams, persons, and players;
- fixtures and match scheduling;
- pre-match lineups;
- match lifecycle and clock;
- C1 live events;
- regular goals, penalty goals, own goals, cards, and substitutions;
- added time and basic match interruption/resumption;
- correction and void workflows;
- deterministic event replay;
- automatic score, timeline, basic statistics, and standings;
- operator assignment and supervisor verification;
- admin command API;
- a minimal operator adapter, initially `footballctl`;
- public REST read API;
- transactional outbox and WebSocket updates;
- audit, idempotency, observability, and data-quality foundations.

### 3.2 Explicitly outside the mandatory first-year core

These features must not be implemented before the C1 platform succeeds in real pilot matches:

- expected goals (`xG`);
- player ratings;
- heatmaps;
- pass maps;
- player tracking;
- full C3 pass, duel, dribble, touch, and pressure capture;
- machine-learning models;
- recommendation systems;
- a separate search cluster;
- microservices created only for anticipated scale;
- NATS created only for anticipated scale;
- Dragonfly or Redis created only for anticipated scale;
- Kubernetes;
- multiple authoritative databases.

### 3.3 Product decisions required before implementation

- [ ] The first competition is named.
- [ ] The first season is named.
- [ ] C1 is confirmed as the first release coverage level.
- [ ] The authoritative data sources are identified.
- [ ] Operator and supervisor responsibilities are identified.
- [ ] Expected simultaneous live matches are identified.
- [ ] Minimum public mobile screens are listed.
- [ ] Supported locales are confirmed.
- [ ] Correction approval responsibility is confirmed.
- [ ] Whether a full web admin panel is required is decided separately from the mandatory admin API.
- [ ] Hosting and production environment decisions are explicitly deferred or confirmed.
- [ ] The buyer approves the first vertical-slice acceptance scenario.

No public behavior or database schema should be treated as final until these decisions are recorded.

---

## 4. Delivery strategy: vertical first, depth second

The first implementation must not build every directory and identity feature before touching the match engine. A thin end-to-end slice must work by the end of the second roadmap month:

```text
minimal administrator identity
→ two teams
→ minimal player registry
→ one competition and season
→ one fixture
→ two lineups
→ start match
→ record goal/card/substitution
→ finish match
→ correct or void one event
→ read current match state through REST
→ operate the flow through footballctl
```

This first slice may support only the minimum fields required by the scenario. Later months harden each bounded context without replacing the architecture.

WebSocket is not required for the first vertical slice. REST snapshots are sufficient until canonical persistence, replay, and correction work reliably.

---

## 5. Non-negotiable architecture principles

### 5.1 Modular monolith

The platform starts as one repository and one modular application system. Modules have strict ownership and dependency rules. A module becomes a separate service only after operational evidence proves that independent deployment or scaling is necessary.

### 5.2 Domain independence

The domain layer must not know about:

- HTTP or Axum;
- SQLx or PostgreSQL;
- WebSocket;
- JSON transport structures;
- environment variables;
- tracing implementations;
- process lifecycle.

The domain layer owns football language, typed IDs, value objects, entities, aggregates, invariants, domain commands, and domain events.

### 5.3 PostgreSQL is authoritative

Cache, WebSocket, an event bus, logs, and external search indexes are never authoritative state.

### 5.4 Partial event sourcing

Append-only event streams are used for:

- live match events;
- match lifecycle transitions;
- clock transitions;
- event corrections and voids;
- critical verification decisions.

Ordinary master data uses relational CRUD with audit and history where required.

### 5.5 Canonical events are immutable

A published canonical event is never updated or deleted in place. A mistake creates a correction or void event. Original evidence remains available.

### 5.6 Derived state is not manually overwritten

The following are reduced from canonical events or verified results:

- current score;
- timeline;
- players on the pitch;
- card state;
- basic match statistics;
- standings;
- player and team summaries.

### 5.7 Commands are idempotent

Every logical mutation has a client command ID and request fingerprint. Repeating the same command ID with the same fingerprint returns the persisted result. Reusing the same command ID with a different fingerprint returns an idempotency conflict.

### 5.8 Optimistic concurrency is explicit

Aggregate commands carry `expected_version`. A stale version returns a conflict rather than silently overwriting newer state.

### 5.9 Every canonical event advances aggregate version

One logical command may emit zero, one, or multiple canonical events. If current version is `v` and the command emits `N` events, those events receive versions `v+1` through `v+N` in deterministic order.

### 5.10 Modules write only their own data

One bounded context must not directly insert, update, or delete another context’s owned tables. Cross-module effects use application orchestration, explicit ports, or versioned integration events.

### 5.11 Time and identifiers are explicit

- Database timestamps use UTC.
- API timestamps use ISO-8601.
- Presentation timezone belongs to the API/client boundary.
- One UUID strategy is selected and applied consistently.
- Domain tests use injected clock and ID-generation ports.
- Business status is represented by typed values, not arbitrary strings.

### 5.12 Complexity requires evidence

No cache, message broker, service split, generic abstraction, or denormalized read model is added merely because it may become useful later.

---

## 6. Target repository structure

The structure is created incrementally. Empty crates and directories must not be added months before they have a real responsibility.

```text
socket/
├── Cargo.toml
├── rust-toolchain.toml
├── .env.example
├── apps/
│   ├── admin-api/
│   ├── public-api/
│   ├── worker/
│   └── footballctl/
├── crates/
│   ├── domain/
│   │   └── src/
│   │       ├── identity/
│   │       ├── directory/
│   │       ├── competition/
│   │       ├── scheduling/
│   │       ├── match_engine/
│   │       ├── standings/
│   │       ├── statistics/
│   │       └── quality/
│   ├── application/
│   │   └── src/
│   │       ├── commands/
│   │       ├── queries/
│   │       ├── services/
│   │       └── ports/
│   ├── infrastructure/
│   │   └── src/
│   │       ├── postgres/
│   │       ├── realtime/
│   │       ├── storage/
│   │       └── security/
│   ├── contracts/
│   ├── platform/
│   │   └── src/
│   │       ├── config/
│   │       ├── observability/
│   │       ├── http/
│   │       └── errors/
│   └── testkit/
├── migrations/
├── fixtures/
├── docs/
├── scripts/
└── deploy/
```

### 6.1 Dependency direction

```text
apps
 ├── application
 ├── infrastructure
 ├── contracts
 └── platform

infrastructure
 ├── application ports
 ├── domain
 └── platform

application
 ├── domain
 └── contracts

domain
 └── standard library and minimal shared primitives
```

Forbidden dependencies:

- `domain -> infrastructure`;
- `domain -> apps`;
- `application -> Axum handler`;
- `public-api -> admin-api`;
- one domain module importing another module’s private internals;
- SQL row structures used directly as public API responses.

### 6.2 Crate granularity

Many tiny crates do not automatically produce a good architecture. Start with coarse-grained crates and strict internal modules. Extract a domain module into a crate only when it has a stable public boundary and a concrete dependency, ownership, compile-time, or deployment reason.

---

## 7. Bounded contexts

### 7.1 Identity

Owns:

- administrators;
- roles and permissions;
- sessions;
- competition access;
- operator identity;
- authenticated audit actors.

Does not own football rules.

### 7.2 Directory

Owns:

- countries, regions, cities, and venues;
- clubs and teams;
- persons and player profiles;
- localized names and aliases;
- squad membership history;
- entity archive and merge history.

Core invariants:

- a club is not a team;
- a person is not a player profile;
- contract, squad membership, and competition registration are different concepts;
- historical membership is not overwritten.

### 7.3 Competition

Owns:

- competitions;
- seasons;
- stages;
- groups;
- rounds;
- ties;
- versioned competition, standings, and discipline rules.

Does not own live match events.

### 7.4 Scheduling

Owns:

- fixtures;
- kickoff time;
- home and away assignment;
- venue assignment;
- postponement and cancellation metadata;
- match officials;
- the contract used to create a match.

### 7.5 Match Engine

Owns:

- match aggregate;
- aggregate version;
- match state machine;
- lineup state;
- players on the pitch;
- match clock;
- C1 event validation;
- corrections and voids;
- deterministic replay.

This is the core domain module.

### 7.6 Standings

Owns:

- season results revision;
- processed results revision;
- played/won/drawn/lost reduction;
- goals and points reduction;
- configurable tiebreakers;
- point adjustments;
- standings positions.

### 7.7 Statistics

Owns:

- basic player and team counters;
- top-scorer and discipline summaries;
- future C2 event definitions;
- future feature extraction for C3 and analytics.

Statistics never owns canonical score.

### 7.8 Quality

Owns:

- verification state;
- evidence references;
- completeness and consistency flags;
- operator correction metrics;
- unresolved review queues.

---

## 8. Module implementation pattern

Each feature is implemented in this order:

```text
1. Vocabulary and invariants
2. Typed IDs and value objects
3. Command or query input
4. Domain behavior
5. Domain event or result
6. Repository/application port
7. Domain unit tests
8. PostgreSQL migration
9. PostgreSQL adapter
10. Application service
11. HTTP or CLI adapter
12. Integration tests
13. OpenAPI/contract update
14. Observability
15. Roadmap update
```

A feature may use this internal layout when the complexity justifies it:

```text
feature/
├── mod.rs
├── model.rs
├── command.rs
├── event.rs
├── rules.rs
├── error.rs
└── tests.rs
```

Do not create all files by default. Keep a feature together until separation improves readability.

---

## 9. Data and command architecture

### 9.1 Master data

Master data uses relational persistence with explicit constraints and audit/history where needed.

Rules:

- foreign keys protect ownership relationships;
- unique constraints mirror true domain invariants;
- status values use explicit enums or check constraints;
- creation actor and timestamp are stored;
- updates store actor and timestamp where required;
- historical records are not overwritten;
- user-visible deletion normally becomes archive/status;
- schema evolution uses forward fixes;
- SQL rows do not escape infrastructure adapters.

### 9.2 Logical commands

A logical command represents one idempotent business intention.

Target metadata:

```text
commands
- id
- client_command_id UNIQUE
- request_fingerprint
- actor_id
- aggregate_type
- aggregate_id
- command_type
- expected_version
- status
- created_at
- started_at
- completed_at
- processing_owner
- processing_expires_at
- final_aggregate_version
- result_reference
- rejection_code
```

Command status semantics must be explicit:

```text
RECEIVED
PROCESSING
ACCEPTED
REJECTED
FAILED_RETRYABLE
```

Rules:

- same command ID and same fingerprint returns the stored result;
- same command ID and different fingerprint returns an idempotency conflict;
- a domain rejection consumes the command ID and returns the same rejection on retry;
- an infrastructure failure is retryable only when no canonical commit occurred;
- command acceptance, canonical events, critical state, audit, and outbox commit atomically;
- recovery behavior for stale `PROCESSING` state is designed before background recovery is implemented.

The initial vertical slice may implement this as a minimal processed-command record. It must preserve the same semantics even if the full `commands` table is introduced later.

### 9.3 Optional command attempts

Transport attempts are observability records, not domain events.

```text
command_attempts
- id
- command_id
- received_at
- transport
- result
- rejection_code
- latency_ms
- request_hash
```

Rules:

- storing complete rejected payloads is not required;
- request hashes, error codes, and safe metadata are normally sufficient;
- sensitive data is never copied unnecessarily;
- retention and partitioning are defined before high-volume use;
- this table is optional until real operator analysis requires it.

### 9.4 Canonical match events

Canonical events represent accepted football reality.

```text
match_events
- event_id
- match_id
- aggregate_version
- causation_command_id
- event_index
- event_type
- event_schema_version
- period
- clock_ms
- occurred_at
- recorded_at
- operator_id
- verification_status
- payload
- supersedes_event_id
- voids_event_id
```

Required constraints:

```text
UNIQUE (match_id, aggregate_version)
UNIQUE (causation_command_id, event_index)
```

Rules:

- one command may emit multiple events;
- every event advances aggregate version by one;
- `event_index` starts at zero and orders events from one command;
- event types and payload schema are versioned;
- frequently queried metadata remains in typed columns;
- event-specific data uses typed payloads;
- deserialized events are validated before replay;
- accepted canonical events are immutable.

### 9.5 Command admission and atomic domain execution

Command admission is a short transaction that establishes idempotency before football state is changed:

```text
1. insert or load command by client_command_id
2. compare request fingerprint
3. return the stored result when the command is terminal
4. reject or report an active PROCESSING lease
5. claim RECEIVED, FAILED_RETRYABLE, or expired PROCESSING command
6. set processing_owner and processing_expires_at
7. commit admission
```

The claimed command is then executed in the domain transaction:

```text
1. lock the claimed command row
2. verify processing ownership and lease
3. lock/load aggregate stream or current row
4. validate expected_version
5. execute domain command
6. assign v+1 ... v+N to emitted events
7. write canonical events
8. update critical synchronous state
9. write audit entry
10. write outbox messages
11. persist ACCEPTED result and final version
12. commit
```

A final domain rejection commits `REJECTED`, its stable rejection code, and required audit without canonical match events. A retryable infrastructure failure must not claim success; an expired processing lease allows safe retry. If commit outcome is uncertain, the system queries command/event state by command ID before attempting execution again.

No integration event is published before the domain transaction commits successfully.

### 9.6 Projection and snapshot rules

The event stream is authoritative for match behavior. Read projections exist for efficient queries.

Synchronous state in the command transaction:

- canonical events;
- current match status;
- current score;
- current clock state;
- players on the pitch;
- current aggregate version;
- minimal timeline row or timeline head;
- audit;
- outbox.

Asynchronous projections:

- standings;
- leaderboards;
- competition form;
- aggregated statistics;
- quality aggregates;
- notifications;
- search documents;
- future analytics.

Snapshots are introduced only after replay performance is measured. A snapshot is never authoritative.

---

## 10. Match engine model

### 10.1 Initial state machine

```text
DRAFT
  → SCHEDULED
  → PRE_MATCH
  → LINEUPS_PROVISIONAL
  → LINEUPS_CONFIRMED
  → FIRST_HALF
  → HALF_TIME
  → SECOND_HALF
  → FINISHED
```

Early operational extensions:

```text
live period → INTERRUPTED → previous live period
SCHEDULED/PRE_MATCH → POSTPONED
SCHEDULED/PRE_MATCH → CANCELLED
```

Later extensions:

```text
DELAYED
ABANDONED
AWARDED
EXTRA_TIME_FIRST_HALF
EXTRA_TIME_BREAK
EXTRA_TIME_SECOND_HALF
PENALTY_SHOOTOUT
```

Every new transition is introduced by a separate task with valid and invalid transition tests.

### 10.2 Initial commands

```text
ScheduleMatch
SubmitLineup
ConfirmLineup
StartFirstHalf
EndFirstHalf
StartSecondHalf
DeclareAddedTime
PauseClock
ResumeClock
CorrectClock
InterruptMatch
ResumeInterruptedMatch
RecordGoal
RecordYellowCard
RecordSecondYellowCard
RecordRedCard
RecordSubstitution
EndMatch
CorrectEvent
VoidEvent
VerifyMatch
ReopenMatch
```

### 10.3 Clock

The database is not updated every second.

Stored clock state:

```text
period
base_clock_ms
anchor_server_time
is_running
added_time_ms
last_corrected_at
version
```

Current display time is calculated from base time and the server anchor. Domain code uses an injected clock port.

Clock pause is not the same as match interruption. Interruption is an audited match lifecycle decision with a reason; clock pause is a timing action.

### 10.4 Goal classification

C1 stores only the award semantics required to calculate score correctly:

```rust
enum GoalAwardType {
    Regular,
    Penalty,
    OwnGoal,
}
```

Later C2 context is represented independently:

```rust
enum GoalSituation {
    OpenPlay,
    Corner,
    DirectFreeKick,
    IndirectFreeKick,
    OtherSetPiece,
}

struct GoalContext {
    award_type: GoalAwardType,
    situation: Option<GoalSituation>,
    is_counter_attack: Option<bool>,
}
```

`OpenPlay`, set-piece origin, and counterattack are not mixed into the C1 award type. A direct throw-in goal is not modeled as a valid scoring situation; later event-sequence context may still record a possession that began with a throw-in.

### 10.5 Event granularity

One real football action should not become multiple source events that double-count statistics.

Future examples:

```text
SHOT { outcome: goal|saved|off_target|blocked|woodwork }
PASS { outcome: completed|failed }
DUEL { winner, loser, duel_type }
```

`SHOT_ON_TARGET`, `PASS_COMPLETED`, and `DUEL_WON` are projections or attributes, not separate canonical events for the same action.

---

## 11. Standings consistency

Standings are asynchronous because one season table depends on multiple verified match results.

Each season owns a monotonically increasing `results_revision`.

Rules:

1. a final or corrected verified result increments the season’s `results_revision` atomically with its outbox message;
2. the standings worker processes result changes in order or rebuilds deterministically;
3. standings store `computed_results_revision`;
4. the projection is current only when both revisions match;
5. public responses disclose projection freshness.

Example metadata:

```json
{
  "standings_revision": 57,
  "computed_at": "2026-08-08T12:30:00Z",
  "source_results_revision": 57,
  "is_rebuilding": false
}
```

Position is computed from the season’s versioned standings rules, not hard-coded SQL ordering.

---

## 12. API architecture

### 12.1 Admin command API

Every admin mutation requires:

- authenticated actor;
- permission check;
- request ID;
- client command ID/idempotency key;
- request fingerprint;
- expected aggregate version where applicable;
- structured validation errors;
- atomic audit;
- reason for correction, void, interruption, or reopen.

### 12.2 Operator adapters

Classification:

```text
Admin command API       mandatory
Minimal footballctl     mandatory before first vertical-slice acceptance
Keyboard-first console  optional until pilot workflow requires it
Full web admin panel    optional presentation layer
```

All adapters call the same application commands. No adapter may duplicate football rules.

### 12.3 Public read API

Public endpoints read projections, never admin write models directly.

```json
{
  "data": {},
  "meta": {
    "version": 1,
    "generated_at": "UTC timestamp",
    "verification_status": "provisional|verified",
    "quality_flags": []
  }
}
```

Conventions:

- public version prefix: `/v1`;
- admin prefix: `/admin/v1`;
- stable machine-readable error codes;
- field-level validation details;
- cursor pagination for large lists;
- strict maximum limits;
- allowlisted filters and sorting;
- ISO-8601 UTC timestamps;
- OpenAPI updated with implementation;
- persistence rows never used as transport contracts.

### 12.4 Real-time protocol

WebSocket distributes versioned notifications or patches. Canonical snapshots remain available through REST.

```text
topic: match:{match_id}
sequence: monotonic per topic
event type: versioned
payload: minimal notification or patch
```

Reconnect flow:

1. client sends its last sequence;
2. server returns retained messages when available;
3. a gap beyond retention returns `snapshot_required`;
4. client fetches canonical REST state;
5. duplicate messages remain safe.

---

## 13. Security baseline

- Admin and public routes are logically separated.
- Password and token cryptography are never handwritten.
- Access sessions are short-lived.
- Refresh/session rotation is explicit.
- RBAC uses resource/action permissions.
- Operators write only to assigned competitions or matches.
- Request-body limits are enforced.
- Public and admin rate limits are independent.
- Secrets never enter the repository.
- `.env.example` contains names, not credentials.
- Sensitive payloads are not copied into logs or command-attempt records.
- Business mutation and audit commit atomically.
- Operators never receive direct production database access.
- Correction, interruption, void, and reopen require reasons.

---

## 14. Testing strategy

### 14.1 Domain unit tests

Each invariant receives a fast database-free test.

Examples:

- one player cannot appear twice in a lineup;
- a player not on the pitch cannot score;
- a substituted-out player cannot participate in later live events;
- a second yellow creates red-card state;
- an own goal increments the opponent’s score;
- a penalty goal increments normal score;
- a voided goal rebuilds score correctly;
- an interrupted match cannot accept ordinary live events until resumed;
- duplicate command execution does not change state;
- replay is deterministic.

### 14.2 Application tests

Using fake ports:

- permissions;
- idempotency fingerprint matching;
- same-key/different-payload conflict;
- expected-version conflict;
- transaction orchestration;
- audit creation;
- retryable versus final rejection;
- repository error mapping.

### 14.3 PostgreSQL integration tests

- migrations on a clean database;
- constraints protecting invariants;
- repository round trips;
- multi-event command persistence;
- concurrent aggregate updates;
- duplicate command handling;
- outbox atomicity;
- worker locking;
- season results revision.

### 14.4 Contract tests

- HTTP status codes;
- error and response schemas;
- authentication requirements;
- idempotency behavior;
- pagination;
- OpenAPI compatibility.

### 14.5 Golden match tests

A realistic event stream is replayed and compared with a golden projection.

### 14.6 Property tests

- score never becomes negative;
- event versions are contiguous;
- replay is deterministic;
- duplicate commands do not mutate state twice;
- correction chains cannot form cycles;
- standings revisions never move backward.

### 14.7 Verification hierarchy

After each task, run the narrowest relevant command first:

```text
1. changed module unit test
2. changed crate tests
3. relevant integration tests
4. workspace tests
5. lint
6. format check
7. build
```

Exact commands are recorded after the Cargo workspace and scripts actually exist. Commands are never invented before repository evidence exists.

---

## 15. Observability and operations

Foundation requirements:

- structured logs;
- request ID;
- client command ID;
- actor ID;
- match ID where applicable;
- stable error code;
- latency;
- secret and sensitive-data redaction.

Later metrics:

- HTTP latency and error rate;
- database query latency;
- match command latency;
- operator event delay;
- outbox pending count and oldest age;
- projection lag;
- season standings revision lag;
- active WebSocket connections;
- operator correction rate;
- match quality score.

Health endpoints:

- liveness: process is alive;
- readiness: required dependencies are ready;
- dependency details are not exposed publicly.

---

## 16. Definition of Ready

A task may start only when:

- [ ] It has one clear business outcome.
- [ ] Its owning module is known.
- [ ] Inputs and outputs are known.
- [ ] Invariants are written.
- [ ] Error cases are written.
- [ ] Database impact is known.
- [ ] API-contract impact is known.
- [ ] An acceptance test can be written.
- [ ] It fits one small verified increment.

## 17. Definition of Done

A task is complete only when:

- [ ] Module boundaries remain intact.
- [ ] Domain invariants are tested.
- [ ] Required migrations exist.
- [ ] Required PostgreSQL integration tests exist.
- [ ] API contracts are updated where relevant.
- [ ] Audit and security requirements are satisfied.
- [ ] Errors are structured and observable.
- [ ] Relevant verification passes.
- [ ] No dead code or fake fallback remains.
- [ ] The roadmap checkbox is updated from evidence.
- [ ] No hidden blocker is passed to the next task.

---

## 18. One-year implementation roadmap

This roadmap contains 52 ordered weekly blocks. Week numbers express dependency order, not guaranteed calendar duration. A blocked or incomplete gate delays all dependent work.

## Month 1 — Scope, architecture, and thin platform foundation

### Week 1 — Product contract and vocabulary

- [x] `P0-001` Name the first competition and season.
- [x] `P0-002` Confirm C1 as the first release coverage.
- [x] `P0-003` Identify operator, supervisor, and administrator roles.
- [x] `P0-004` List the minimum public mobile screens.
- [x] `P0-005` Define club, team, squad, lineup, fixture, match, stage, round, and tie.
- [x] `P0-006` Confirm the initial C1 commands and events.
- [x] `P0-007` Write the first end-to-end acceptance scenario.
- [x] `P0-008` Record explicit first-release exclusions.

Gate: the complete path from fixture creation to verified result is understandable to the buyer and developer.
Gate status: Passed on 2026-08-08. Product authority, roles, clients/locales, minimum public screens, canonical vocabulary, C1 commands/events, acceptance flow, and first-release exclusions are recorded in `docs/decisions/P0-001-product-contract.md`, `P0-004-mobile-screen-scope.md`, `P0-005-domain-vocabulary.md`, and `P0-006-c1-commands-events.md`.

### Week 2 — Architecture decisions

- [x] `ARC-001` Confirm the modular-monolith boundary.
- [x] `ARC-002` Confirm partial event sourcing for match behavior.
- [x] `ARC-003` Select the UUID strategy.
- [x] `ARC-004` Confirm UTC and timezone rules.
- [x] `ARC-005` Define the error taxonomy.
- [x] `ARC-006` Define API versioning rules.
- [x] `ARC-007` Define forward-only migration policy.
- [x] `ARC-008` Freeze command/event/version semantics from Section 9.
- [x] `ARC-009` Freeze synchronous/asynchronous projection boundaries.

Gate: implementation does not need to guess foundational semantics.
Gate status: Passed on 2026-08-08. Modular boundary, partial event sourcing, UUID strategy, UTC/timezone rules, error taxonomy, API versioning, migration policy, command/event/version semantics, and projection boundaries are recorded in `docs/decisions/ARC-001-modular-monolith-boundary.md` through `ARC-009-projection-boundaries.md`.

### Week 3 — Cargo and PostgreSQL skeleton

- [x] `FND-001` Inspect repository and source-control ownership before initialization.
- [x] `FND-002` Create the root Cargo workspace.
- [x] `FND-003` Create only the domain, application, infrastructure, contracts, platform, and testkit crates needed by the first slice.
- [x] `FND-004` Create the minimal admin API binary.
- [x] `FND-005` Define Rust toolchain policy.
- [x] `FND-006` Define formatting and lint policy.
- [x] `DB-001` Define the local PostgreSQL connection contract.
- [x] `DB-002` Configure the connection pool.
- [x] `DB-003` Establish migration naming and execution.
- [x] `DB-004` Add clean-database migration verification.

Gate: the empty vertical skeleton builds, tests, connects to PostgreSQL, and applies migrations.
Gate status: Passed on 2026-08-09. The clean migration fixture and all current
PostgreSQL integration fixtures ran against dedicated project test databases;
existing databases were not modified.

### Week 4 — Configuration, errors, health, and minimal actor

- [x] `FND-008` Implement typed configuration.
- [x] `FND-009` Validate required environment values.
- [x] `FND-010` Redact secrets from debug output.
- [x] `FND-011` Implement shared application errors.
- [x] `FND-012` Map application errors to HTTP.
- [x] `FND-013` Add structured logging.
- [x] `FND-014` Add request IDs.
- [x] `FND-015` Add liveness and database readiness.
- [x] `IDN-001` Define `AdminUserId` and minimal actor identity.
- [x] `IDN-002` Persist one active administrator for the vertical slice.

Gate: the process fails clearly on invalid configuration and can authenticate or inject one controlled development administrator without bypassing application boundaries.
Gate status: Passed on 2026-08-14. A dedicated local database was created for
this increment; the clean migration test passed, and the guarded
`footballctl admin seed-development` command passed insert, read-back, repeat,
active-state, and conflicting-identity checks through PostgreSQL.

## Month 2 — First working match vertical slice

### Week 5 — Minimal directory, competition, fixture, and lineup

- [x] `DIR-001` Define minimal team identity and name.
- [x] `DIR-002` Persist two teams.
- [x] `DIR-003` Define minimal person/player identity.
- [x] `DIR-004` Persist players assigned to teams.
- [x] `CMP-001` Define minimal competition and season.
- [x] `SCH-001` Define one fixture with home, away, and kickoff.
- [x] `LIN-001` Define starter and substitute entries.
- [x] `LIN-002` Validate that one player cannot appear twice.
- [x] `LIN-003` Persist and load a draft lineup.

Gate: one valid fixture with two valid lineups can be created through application services.
Gate status: Persistence and repository evidence passed on 2026-08-14 on a
dedicated clean PostgreSQL database. The remaining full Week 5 gate is the
application-service composition itself; it is not marked complete by these
repository-level fixtures.

### Week 6 — Match aggregate and clock

- [x] `MCH-001` Create match identity from a fixture.
- [x] `MCH-002` Define the initial match states.
- [x] `MCH-003` Define the `MatchAggregate` boundary.
- [x] `MCH-004` Define aggregate version semantics.
- [x] `MCH-005` Define command result and domain event envelopes.
- [x] `MCH-006` Implement pre-match and lineup-confirmed transitions.
- [x] `CLK-001` Define period and clock value objects.
- [x] `CLK-002` Implement start, pause, resume, and correction.
- [x] `CLK-003` Use an injected clock port.
- [x] `MCH-007` Implement first-half, half-time, second-half, and finish transitions.

Gate: a database-free aggregate test runs a match from confirmed lineups to finished state.

### Week 7 — Logical command store, event store, and C1 actions

- [x] `CMD-001` Define logical command identity and request fingerprint.
- [x] `CMD-002` Define final rejection and retryable failure semantics.
- [x] `CMD-003` Implement the short command-admission transaction.
- [x] `CMD-004` Reject same command ID with a different fingerprint.
- [x] `CMD-007` Persist accepted or rejected terminal command results.
- [x] `EVT-001` Create the canonical match-event migration.
- [x] `EVT-002` Implement event schema versioning.
- [x] `EVT-003` Load an event stream.
- [x] `EVT-004` Append multiple events with expected version.
- [x] `EVT-005` Persist causation command ID and event index.
- [x] `EVT-006` Implement goal, yellow/red card, and substitution events.
- [x] `PRJ-001` Implement synchronous score and timeline reduction.
- [x] `EVT-007` Test duplicate and concurrent commands.

Gate: canonical C1 events persist atomically and replay to the same state.
Gate status: Passed on 2026-08-09. Dedicated clean-database fixtures verified
canonical append/load, terminal-result evidence, typed payload round-trip,
expected-version concurrency, duplicate-command admission, and the
application-orchestrator fingerprint-conflict path. The application admission
fixture also verified matching terminal replay returns the persisted result
without invoking execution. The C1 application execution fixture additionally
verified accepted event plus terminal-result commit and deterministic rejected
command persistence without a second canonical event, including the matching
accepted/rejected command-audit rows.

### Week 8 — Admin API, footballctl, correction, and public REST

- [ ] `APP-001` Implement load-execute-commit command orchestration.
- [ ] `APP-002` Integrate expected version, idempotency, and audit.
- [x] `API-001` Add lineup, start-period, goal, card, substitution, and finish endpoints.
- [x] `CLI-001` Create `footballctl` configuration and authentication adapter.
- [ ] `CLI-002` Add fixture, lineup, start, goal, card, substitution, and finish commands.
- [x] `COR-001` Implement correction reason and void event.
- [x] `COR-002` Implement an effective event-stream builder.
- [x] `COR-003` Rebuild score after a voided goal.
- [x] `PUB-001` Add minimal match-detail and timeline REST endpoints.
- [ ] `PIL-001` Execute the complete vertical-slice acceptance fixture.

Gate: one operator can run the full match through `footballctl`, correct an event, and read canonical current state through REST.
Gate status: CLI-001 passed on 2026-08-14. `footballctl` now uses one typed
configuration/database adapter and centralized development authorization
boundaries; exact authorization and configuration failure behavior is covered
by unit and subprocess tests. APP-001 and APP-002 remain open because their
updated clean-database acceptance has not yet been run. Their C1 executor now
publishes correction-resolved score/timeline roots in the canonical command
transaction together with audit, outbox, and terminal result. The canonical
lifecycle executor now applies the same transaction to schedule, lineup
submission/confirmation, period transitions, finish, and synchronous
summary/timeline/lineup publication. HTTP and CLI transport exposure remains
part of this week's API/CLI work.
PUB-001 passed on 2026-08-31: the separate public API serves canonical
match-detail and timeline resources from the correction-resolved event stream,
with versioned DTOs, stable errors, and an in-repository OpenAPI contract.
API-001 passed on 2026-08-31: ten versioned admin command routes now cover
schedule, lineup submit/confirm, regular period transitions, finish, goals,
cards, and substitutions through the canonical command executors. Strict
request/response contracts, development identity and active-operator lease
guards, stable HTTP errors, and the admin OpenAPI surface are tested without
implicitly mutating a database.

## Month 3 — Directory and localization hardening

### Week 9 — Geography and venues

- [ ] `DIR-008` Model country, region, and city.
- [ ] `DIR-009` Add geography migrations and repositories.
- [ ] `DIR-010` Add create/update/archive commands.
- [ ] `DIR-011` Model venues and capacity/location validation.
- [ ] `DIR-012` Add venue persistence.
- [ ] `DIR-013` Add admin endpoints and contract tests.

### Week 10 — Club and team lifecycle

- [ ] `DIR-014` Model club identity and status.
- [ ] `DIR-015` Add club history rules.
- [ ] `DIR-016` Add club migration and repository.
- [ ] `DIR-017` Model team type and club-team relationship.
- [ ] `DIR-018` Add team migration and repository.
- [ ] `DIR-019` Add create/update/archive commands.
- [ ] `DIR-020` Test that club and team remain distinct concepts.

### Week 11 — Person and player profile

- [ ] `DIR-021` Model person identity and names.
- [ ] `DIR-022` Define birth-data validation.
- [ ] `DIR-023` Add person migration and repository.
- [ ] `DIR-024` Model player profile.
- [ ] `DIR-025` Define preferred position, preferred foot, and shirt number values.
- [ ] `DIR-026` Add player profile persistence.
- [ ] `DIR-027` Add duplicate-candidate query.

### Week 12 — Localization and membership history

- [ ] `LOC-001` Define locale values.
- [ ] `LOC-002` Model localized names and aliases.
- [ ] `LOC-003` Define normalized search names.
- [ ] `LOC-004` Add localization migration and uniqueness rules.
- [ ] `LOC-005` Define locale fallback.
- [ ] `MEM-001` Model squad membership intervals.
- [ ] `MEM-002` Prevent invalid membership overlap.
- [ ] `MEM-003` Add historical squad queries.

Gate: clubs, teams, persons, players, and historical squad membership are queryable in confirmed locales.

## Month 4 — Competition and scheduling hardening

### Week 13 — Competition and season lifecycle

- [ ] `CMP-002` Add competition types and status lifecycle.
- [x] `CMP-003` Add competition migration and repository.
- [ ] `CMP-004` Add competition admin commands.
- [ ] `CMP-005` Model season date interval.
- [x] `CMP-006` Add season migration and repository.
- [ ] `CMP-007` Define overlapping-season policy.

### Week 14 — Stage, group, round, and rules

- [ ] `CMP-008` Model ordered stages.
- [ ] `CMP-009` Model groups and rounds.
- [ ] `CMP-010` Add stage/group/round persistence.
- [ ] `RUL-001` Model points rules.
- [ ] `RUL-002` Model ordered tiebreakers.
- [ ] `RUL-003` Version competition rules.
- [ ] `RUL-004` Make published season rules immutable.

### Week 15 — Registration and fixture lifecycle

- [ ] `REG-001` Model team season registration.
- [ ] `REG-002` Model player competition registration.
- [ ] `REG-003` Add eligibility queries.
- [ ] `SCH-002` Harden home/away and kickoff invariants.
- [ ] `SCH-003` Add venue and officials assignment.
- [ ] `SCH-004` Add reschedule and postponement history.
- [ ] `SCH-005` Add cancellation reason and state.

### Week 16 — Added time and match interruption

- [ ] `CLK-004` Model added time.
- [ ] `CLK-005` Implement `DeclareAddedTime`.
- [ ] `MCH-008` Model interruption reason.
- [ ] `MCH-009` Implement `InterruptMatch`.
- [ ] `MCH-010` Implement `ResumeInterruptedMatch`.
- [ ] `MCH-011` Reject normal live actions while interrupted.
- [ ] `MCH-012` Add interruption audit and API endpoints.
- [ ] `MCH-013` Add lifecycle golden tests.

Gate: the initial league model and realistic C1 match lifecycle support ordinary operational exceptions.

## Month 5 — Identity, authorization, operators, and verification

### Week 17 — Administrator authentication

- [ ] `IDN-003` Model administrator status.
- [ ] `IDN-004` Add administrator repository.
- [ ] `IDN-005` Add credential policy and hashing adapter.
- [ ] `IDN-006` Implement login.
- [ ] `IDN-007` Persist sessions.
- [ ] `IDN-008` Implement access and refresh session lifecycle.
- [ ] `IDN-009` Implement logout and revoke.

### Week 18 — RBAC, audit, and full idempotency

- [ ] `IDN-010` Model roles and permissions.
- [ ] `IDN-011` Add user-role assignment.
- [ ] `IDN-012` Add competition-level access.
- [ ] `IDN-013` Implement permission checking.
- [ ] `AUD-001` Harden audit schema and actor metadata.
- [ ] `AUD-002` Verify atomic business audit.
- [ ] `CMD-005` Add persisted response retrieval for duplicate commands.
- [x] `CMD-006` Add stale-processing recovery semantics.

### Week 19 — Operator assignment and lease

- [x] `OPS-001` Model match assignment.
- [x] `OPS-002` Model operator lease.
- [x] `OPS-003` Implement claim.
- [x] `OPS-004` Implement heartbeat.
- [x] `OPS-005` Implement release and takeover.
- [x] `OPS-006` Define lease expiry.
- [x] `OPS-007` Add two-operator concurrency tests.

### Week 20 — Verification and evidence minimum

- [x] `QUA-001` Model provisional and verified state.
- [x] `QUA-002` Enforce supervisor permissions.
- [x] `QUA-003` Implement match verification.
- [x] `QUA-004` Implement reasoned match reopen.
- [x] `SRC-001` Model a safe evidence reference.
- [x] `QUA-005` Audit correction and verification decisions.
- [x] `QUA-006` Add unauthorized-workflow tests.

Gate: authenticated operators manage only assigned matches and supervisors verify or reopen results with evidence and audit.

## Month 6 — Read models, standings, and public API expansion

### Week 21 — Match read models

- [x] `PRJ-002` Create match-summary projection schema.
- [x] `PRJ-003` Create timeline projection schema.
- [x] `PRJ-004` Create lineup projection schema.
- [x] `PRJ-005` Store projection and aggregate versions.
- [x] `PRJ-006` Implement full match projection rebuild.
- [x] `PRJ-007` Add event/projection consistency tests.

### Week 22 — Standings engine and revisions

- [x] `STD-001` Define verified-result input contract.
- [x] `STD-002` Reduce played, won, drawn, and lost.
- [x] `STD-003` Reduce goals and points.
- [x] `STD-004` Implement configurable tiebreak comparison.
- [ ] `STD-005` Add season `results_revision`.
- [ ] `STD-006` Store `computed_results_revision` in standings.
- [ ] `STD-007` Rebuild standings after corrected result.
- [ ] `STD-008` Add league golden tests.

### Week 23 — Public match endpoints

- [ ] `PUB-002` List matches by date.
- [ ] `PUB-003` List live matches.
- [ ] `PUB-004` Expand match detail.
- [ ] `PUB-005` Add lineup endpoint.
- [ ] `PUB-006` Add basic match statistics endpoint.
- [ ] `PUB-007` Expose verification and quality metadata.
- [ ] `PUB-008` Add pagination and filter contract tests.

### Week 24 — Public competition and directory endpoints

- [ ] `PUB-009` Add competition list and detail.
- [ ] `PUB-010` Add season fixtures and results.
- [ ] `PUB-011` Add season standings with freshness metadata.
- [ ] `PUB-012` Add team profile and squad.
- [ ] `PUB-013` Add player profile.
- [ ] `PUB-014` Add locale selection and fallback.
- [ ] `PUB-015` Record query performance baseline.

Gate: the mobile client can obtain all core C1 state from stable public read contracts.

## Month 7 — Imports, evidence, quality, and search

### Week 25 — CSV import staging

- [ ] `IMP-001` Model import jobs.
- [ ] `IMP-002` Store upload metadata.
- [ ] `IMP-003` Create staging tables.
- [ ] `IMP-004` Parse and normalize rows.
- [ ] `IMP-005` Produce structured validation errors.
- [ ] `IMP-006` Produce import previews.
- [ ] `IMP-007` Approve and commit atomically.

### Week 26 — Duplicate detection and review

- [ ] `IMP-008` Detect person duplicate candidates.
- [ ] `IMP-009` Detect team duplicate candidates.
- [ ] `IMP-010` Create a manual review queue.
- [ ] `SRC-002` Add source types and reliability level.
- [ ] `SRC-003` Link critical corrections to evidence.
- [ ] `SRC-004` Add evidence audit tests.

### Week 27 — Match quality

- [ ] `QUA-007` Create match-quality records.
- [ ] `QUA-008` Store coverage level.
- [ ] `QUA-009` Detect missing lineup.
- [ ] `QUA-010` Detect unverified result.
- [ ] `QUA-011` Detect score conflict.
- [ ] `QUA-012` Expose quality flags publicly.
- [ ] `QUA-013` Add supervisor review queries.

### Week 28 — Search and locale hardening

- [ ] `SEA-001` Define normalized search documents in PostgreSQL.
- [ ] `SEA-002` Add trigram/full-text indexes only from measured queries.
- [ ] `SEA-003` Search clubs, teams, persons, and competitions.
- [ ] `SEA-004` Rank official names above aliases.
- [ ] `SEA-005` Test Latin, Cyrillic, Russian, and English variants.
- [ ] `LOC-006` Audit missing translations and fallback behavior.

Gate: bulk setup, duplicate review, evidence, and quality workflows support realistic data operations.

## Month 8 — Transactional outbox and real-time delivery

### Week 29 — Transactional outbox

- [x] `OUT-001` Create outbox schema.
- [x] `OUT-002` Define versioned integration-event envelope.
- [x] `OUT-003` Insert outbox messages in command transactions.
- [x] `OUT-004` Implement worker claim with safe locking.
- [x] `OUT-005` Define retry and backoff.
- [x] `OUT-006` Define poison-message handling.
- [x] `OUT-007` Test commit and publish failures.

### Week 30 — WebSocket protocol

- [x] `RT-001` Define topic names.
- [x] `RT-002` Implement monotonic per-topic sequence.
- [x] `RT-003` Define subscribe and unsubscribe contracts.
- [x] `RT-004` Define topic authorization.
- [x] `RT-005` Publish match notifications.
- [x] `RT-006` Make duplicate delivery safe.
- [x] `RT-007` Add connection heartbeat and limits.

### Week 31 — Reconnect and synchronization

- [x] `RT-008` Accept last-known sequence.
- [x] `RT-009` Define retained-message window.
- [x] `RT-010` Detect gaps.
- [x] `RT-011` Return snapshot-required response.
- [x] `RT-012` Integrate canonical REST snapshot.
- [x] `RT-013` Add reconnect integration tests.

### Week 32 — Corrections and projection delivery

- [x] `RT-014` Distribute correction notifications.
- [x] `RT-015` Distribute verification changes.
- [x] `RT-016` Distribute standings revision changes.
- [x] `RT-017` Test worker restart and duplicate delivery.
- [x] `RT-018` Test correction during a live match.
- [x] `RT-019` Measure end-to-end fanout latency.

Gate: committed changes reach clients safely, and reconnect cannot silently lose canonical state.

## Month 9 — Operator tooling and workflow hardening

### Week 33 — footballctl setup workflows

- [x] `CLI-003` Add competition and season commands.
- [x] `CLI-004` Add team and player import commands.
- [x] `CLI-005` Add fixture scheduling commands.
- [x] `CLI-006` Add assignment and claim commands.
- [ ] `CLI-007` Add correction, void, verification, and replay commands.
- [ ] `CLI-008` Add safe output and stable exit codes.
- [ ] `CLI-009` Add CLI integration smoke tests.

### Week 34 — Operator ergonomics contract

- [ ] `UX-001` Measure command count for one live match.
- [ ] `UX-002` Define keyboard-first event entry requirements.
- [ ] `UX-003` Define reconnect and offline-error presentation.
- [ ] `UX-004` Define visible lease and version-conflict behavior.
- [ ] `UX-005` Decide whether a console UI is required before pilot.
- [ ] `UX-006` Keep all operator behavior behind the same admin API.

### Week 35 — Admin API completeness

- [ ] `API-002` Complete master-data admin endpoints.
- [ ] `API-003` Complete competition and scheduling endpoints.
- [ ] `API-004` Complete operator lease endpoints.
- [ ] `API-005` Complete correction and verification endpoints.
- [ ] `API-006` Enforce body limits and idempotency headers.
- [ ] `API-007` Complete OpenAPI error contracts.

### Week 36 — Full workflow simulation

- [ ] `SIM-001` Seed a realistic competition fixture.
- [ ] `SIM-002` Assign an operator and supervisor.
- [ ] `SIM-003` Run a match with added time and interruption.
- [ ] `SIM-004` Inject duplicate and stale-version commands.
- [ ] `SIM-005` Correct and verify final result.
- [ ] `SIM-006` Validate REST and WebSocket state.
- [ ] `SIM-007` Record usability and latency findings.

Gate: an operator can run a realistic match without curl or direct database access.

## Month 10 — C1 statistics and reliability testing

### Week 37 — Basic C1 statistics

- [ ] `STA-001` Define player match-stat projection.
- [ ] `STA-002` Define team match-stat projection.
- [ ] `STA-003` Reduce goals, own goals, and penalty goals.
- [ ] `STA-004` Reduce cards and substitutions.
- [ ] `STA-005` Calculate minutes played from lineup and events.
- [ ] `STA-006` Rebuild statistics after correction.
- [ ] `STA-007` Expose statistics freshness metadata.

### Week 38 — Competition summaries

- [ ] `STA-008` Build top-scorer summary.
- [ ] `STA-009` Build discipline summary.
- [ ] `STA-010` Build recent-form summary.
- [ ] `STA-011` Rebuild summaries after corrected result.
- [ ] `STA-012` Add season-revision consistency tests.
- [ ] `STA-013` Add public summary endpoints.

### Week 39 — Golden, property, and concurrency tests

- [ ] `TST-001` Add a complete golden match stream.
- [ ] `TST-002` Add replay determinism property tests.
- [ ] `TST-003` Add contiguous-version property tests.
- [ ] `TST-004` Add correction-chain cycle tests.
- [ ] `TST-005` Test two operators on one match.
- [ ] `TST-006` Test same-key/different-payload conflict.
- [ ] `TST-007` Test multi-event command atomicity.

### Week 40 — Failure and performance baseline

- [ ] `TST-008` Test worker crash after claim.
- [ ] `TST-009` Test commit succeeded but publish failed.
- [ ] `TST-010` Test correction during projection rebuild.
- [ ] `TST-011` Test WebSocket disconnect and reconnect.
- [ ] `PERF-001` Measure command latency.
- [ ] `PERF-002` Measure event replay time.
- [ ] `PERF-003` Measure public query latency.
- [ ] `PERF-004` Record limits without adding speculative cache.

Gate: core behavior survives concurrency, retries, correction, replay, and worker failures.

## Month 11 — Operations, security, recovery, and pilot preparation

### Week 41 — Observability

- [ ] `OBS-001` Add HTTP latency and error metrics.
- [ ] `OBS-002` Add database-query visibility.
- [ ] `OBS-003` Add command and operator-delay metrics.
- [ ] `OBS-004` Add outbox lag metrics.
- [ ] `OBS-005` Add standings-revision lag metrics.
- [ ] `OBS-006` Add log-redaction tests.
- [ ] `OBS-007` Define alert thresholds from pilot expectations.

### Week 42 — Security hardening

- [ ] `SEC-001` Enforce admin/public route separation.
- [ ] `SEC-002` Enforce request-body limits.
- [ ] `SEC-003` Implement admin and public rate-limit policies.
- [ ] `SEC-004` Review session rotation and revocation.
- [ ] `SEC-005` Review competition-level access.
- [ ] `SEC-006` Review correction and reopen authorization.
- [ ] `SEC-007` Verify secrets and sensitive data never enter logs.

### Week 43 — Backup and recovery

- [ ] `BKP-001` Define backup ownership and frequency.
- [ ] `BKP-002` Define WAL/PITR target design.
- [ ] `BKP-003` Define off-site storage policy.
- [ ] `BKP-004` Write restore procedure.
- [ ] `BKP-005` Perform or schedule restore verification in the approved environment.
- [ ] `BKP-006` Measure or explicitly defer RPO/RTO targets.

### Week 44 — Pilot preparation

- [ ] `PIL-002` Prepare realistic clubs, teams, and players.
- [ ] `PIL-003` Prepare competition, season, stages, and rounds.
- [ ] `PIL-004` Prepare operator and supervisor accounts.
- [ ] `PIL-005` Prepare support and incident workflow.
- [ ] `PIL-006` Define pilot success metrics.
- [ ] `PIL-007` Define rollback/stop conditions.
- [ ] `PIL-008` Run a complete rehearsal.

Gate: the platform is observable, recoverable, access-controlled, and ready for an approved pilot.

## Month 12 — Pilot, stabilization, release, and Year 2 decision

### Week 45 — Pilot match 1

- [ ] `PIL-009` Run the first approved pilot match.
- [ ] `PIL-010` Measure operator event delay.
- [ ] `PIL-011` Record command rejects and conflicts.
- [ ] `PIL-012` Record corrections and missing events.
- [ ] `PIL-013` Validate public REST state.
- [ ] `PIL-014` Validate WebSocket reconnect.
- [ ] `PIL-015` Produce a pilot incident report.

### Week 46 — Pilot fixes

- [ ] `FIX-001` Rank pilot failures by severity.
- [ ] `FIX-002` Fix only confirmed core correctness issues.
- [ ] `FIX-003` Add regression tests for every correctness fix.
- [ ] `FIX-004` Re-run affected match streams.
- [ ] `FIX-005` Rebuild and compare projections.
- [ ] `FIX-006` Update operator instructions.

### Week 47 — Pilot match 2

- [ ] `PIL-016` Run a second approved pilot match.
- [ ] `PIL-017` Compare latency with pilot 1.
- [ ] `PIL-018` Compare correction rate with pilot 1.
- [ ] `PIL-019` Compare missing-data rate with pilot 1.
- [ ] `PIL-020` Validate supervisor workflow.
- [ ] `PIL-021` Validate standings revision freshness.

### Week 48 — Evidence-based optimization

- [ ] `PERF-005` Profile measured command bottlenecks.
- [ ] `PERF-006` Profile measured query bottlenecks.
- [ ] `PERF-007` Optimize only verified hot paths.
- [ ] `PERF-008` Re-run correctness and performance tests.
- [ ] `PERF-009` Decide whether cache is justified.
- [ ] `PERF-010` Decide whether realtime fanout needs a broker.

### Week 49 — Release candidate

- [ ] `REL-001` Apply migrations to a clean database.
- [ ] `REL-002` Run full workspace verification.
- [ ] `REL-003` Review OpenAPI contracts.
- [ ] `REL-004` Review security checklist.
- [ ] `REL-005` Review data-quality and correction behavior.
- [ ] `REL-006` Record known limitations.
- [ ] `REL-007` Prepare the core MVP acceptance build.

### Week 50 — Operational documentation

- [ ] `REL-008` Write operator runbook.
- [ ] `REL-009` Write supervisor runbook.
- [ ] `REL-010` Write incident procedure.
- [ ] `REL-011` Write restore procedure.
- [ ] `REL-012` Write projection rebuild procedure.
- [ ] `REL-013` Write deployment and rollback prerequisites without deploying.

### Week 51 — Core acceptance

- [ ] `ACC-001` Demonstrate the complete C1 match flow.
- [ ] `ACC-002` Demonstrate duplicate-command safety.
- [ ] `ACC-003` Demonstrate correction and replay.
- [ ] `ACC-004` Demonstrate operator/supervisor authorization.
- [ ] `ACC-005` Demonstrate REST and WebSocket recovery.
- [ ] `ACC-006` Demonstrate standings revision consistency.
- [ ] `ACC-007` Record buyer acceptance or precise gaps.

### Week 52 — Retrospective and Year 2 gate

- [ ] `Y2-001` Analyze operator latency.
- [ ] `Y2-002` Analyze correction rate.
- [ ] `Y2-003` Analyze missing data.
- [ ] `Y2-004` Analyze query and realtime bottlenecks.
- [ ] `Y2-005` Decide whether operators can reliably collect C2 data.
- [ ] `Y2-006` Decide cache from measured evidence.
- [ ] `Y2-007` Decide broker or service extraction from measured evidence.
- [ ] `Y2-008` Write the Year 2 roadmap.

Gate: the core MVP is verified, replayable, observable, recoverable, and evaluated in real operator workflows.

---

## 19. Year 2 candidate backlog

Only after successful C1 pilots:

1. design the C2 event taxonomy;
2. add structured shots with outcomes;
3. add corners, fouls, offsides, and saves;
4. add possession only when collection methodology is reliable;
5. add full player/team statistics;
6. expose quality-based coverage labels;
7. pilot C3 coordinates;
8. build shot maps from verified coordinate data;
9. design versioned momentum;
10. design explainable player ratings;
11. publish calibrated xG only after sufficient labeled data;
12. add cache only for measured bottlenecks;
13. add NATS only for measured fanout/decoupling needs;
14. extract the real-time gateway only for independent scaling or availability.

---

## 20. Service-extraction readiness

A module remains extractable when:

- its public contract is explicit;
- no other module writes its tables;
- integration events are versioned;
- cross-module side effects live in application orchestration;
- outbox delivery exists;
- read models are separate from authoritative write models;
- IDs are stable;
- transport serialization is not the domain model;
- behavior requiring one transaction remains within one consistency boundary.

Valid extraction signals:

- independent scaling requirement;
- independent release cadence;
- different availability target;
- a separate ownership team;
- measured database contention;
- measured real-time fanout pressure.

“It may be useful later” is not an extraction signal.

---

## 21. Forbidden shortcuts

- Put all code in `main.rs`.
- Put all domains in one `models.rs`.
- Use SQL rows as domain entities.
- Write business rules inside HTTP handlers.
- Overwrite score from an operator-provided total.
- Update or delete published canonical events.
- Hide the whole domain model inside JSONB.
- Use untyped strings for IDs and statuses.
- Log an error and silently swallow it.
- Remove foreign keys or true uniqueness constraints for convenience.
- Weaken production invariants for tests.
- Give operators direct production database access.
- Publish WebSocket messages before database commit.
- Store full rejected payloads without a justified retention and privacy policy.
- Build every exceptional football rule before the basic flow is proven.
- Publish ratings or xG without sufficient data.
- Add cache, NATS, Kubernetes, or microservices before measured need.
- Create unused abstractions in the name of “perfect architecture.”

---

## 22. Work-session protocol

For every implementation session:

1. select exactly one unchecked task;
2. confirm the preceding gate;
3. inspect the repository and owning module;
4. find the nearest existing pattern;
5. write invariants and acceptance behavior;
6. make the smallest complete patch;
7. run relevant verification;
8. fix in-scope failures;
9. check off the task only with evidence;
10. begin the next task in a separate session.

Multiple task IDs may be completed together only when they are one inseparable atomic increment.

---

## 23. First task

### `P0-001` — Name the first competition and define the MVP match flow

This is the first task. No production code should be written before it is complete.

Expected decision record:

```text
Competition: <exact name>
Season: <exact season>
Coverage: C1
Authoritative data source: <source>
Operator role: <person/process>
Supervisor role: <person/process>
Public clients: <mobile/web>
Locales: <confirmed list>
```

Acceptance flow:

```text
create competition
→ create season
→ register two teams
→ register players
→ schedule fixture
→ assign operator
→ submit and confirm lineups
→ start first half
→ record goal/card/substitution
→ declare added time if required
→ interrupt/resume if required
→ finish match
→ correct or void an event
→ verify result
→ update standings revision
→ deliver public REST/WebSocket state
```

Acceptance criteria:

- the buyer understands and approves the flow;
- every human role is identified;
- C1 data is confirmed as sufficient for the first release;
- excluded features are written explicitly;
- `ARC-001` may start without guessing product behavior.

---

## 24. Progress summary

| Milestone | Status |
|---|---|
| Product contract | Completed |
| Architecture decisions | Completed |
| First vertical slice | Not started |
| Directory hardening | Not started |
| Competition/scheduling hardening | Not started |
| Identity/operator workflow | Not started |
| Read models/standings | In progress |
| Import/quality | Not started |
| Outbox/realtime | In progress |
| Operator workflow hardening | Not started |
| Reliability/operations | Not started |
| Pilot/core acceptance | Not started |

Update this table and the task checkboxes only after verified work.
