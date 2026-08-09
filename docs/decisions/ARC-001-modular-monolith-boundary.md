# ARC-001 — Modular-monolith boundary

**Status:** Accepted  
**Decision date:** 2026-08-08  
**Depends on:** Product contract gate

## Decision

The first-year platform is one repository and one modular application system. It starts as a modular monolith, not as a collection of independently deployed services.

The initial runtime is composed from application binaries and coarse-grained crates:

```text
apps
 ├── admin-api
 ├── public-api
 ├── worker
 └── footballctl

crates
 ├── domain
 ├── application
 ├── infrastructure
 ├── contracts
 ├── platform
 └── testkit
```

Only modules with a concrete responsibility are created. Empty future crates, placeholder services, and speculative infrastructure are not added.

## Ownership boundaries

- `domain` owns football vocabulary, typed values, aggregates, invariants, domain commands, and canonical domain events.
- `application` owns use-case orchestration, transaction coordination, authorization ports, and query/command services.
- `infrastructure` owns PostgreSQL adapters, storage, realtime delivery, and security implementations.
- `contracts` owns transport DTOs and versioned external contracts; transport types do not become domain entities.
- `platform` owns configuration, HTTP wiring, observability, shared errors, and process-level concerns.
- `apps` compose the crates into adapters and processes; business rules are not duplicated in handlers or CLI commands.

Each bounded context owns its own tables and persistence behavior. Cross-module effects use application orchestration, explicit ports, or versioned integration events. One module must not directly write another module’s tables.

## Dependency direction

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

- `domain → infrastructure`;
- `domain → apps`;
- `application → Axum handler`;
- `public-api → admin-api`;
- a domain module importing another module’s private internals;
- SQL row structures used as public API responses.

The domain layer does not know about Axum, SQLx, PostgreSQL, WebSocket, JSON transport, environment variables, or process lifecycle.

## Deployment boundary

The modular monolith may run multiple binaries for operational separation, but all first-year authoritative state remains in the same PostgreSQL consistency boundary. A separate service is not introduced merely because independent deployment might be useful later.

## Extraction rule

A module becomes a service candidate only after its public contract, table ownership, versioned integration events, outbox delivery, stable IDs, and separate read models exist. Valid extraction signals are measured independent scaling, release cadence, availability, ownership, database contention, or realtime fanout pressure.

## Acceptance criteria

ARC-001 is complete when a new feature can identify one owning module, one allowed dependency direction, and one persistence boundary without creating a speculative service or bypassing application orchestration.

