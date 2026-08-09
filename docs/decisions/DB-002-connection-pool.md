# DB-002 — PostgreSQL connection pool

**Status:** Accepted  
**Decision date:** 2026-08-08  
**Depends on:** [DB-001 local PostgreSQL contract](DB-001-local-postgres-contract.md)

## Initial pool configuration

The infrastructure layer owns one shared `sqlx::PgPool` per process. The initial local/development defaults are deliberately small and are configuration defaults, not measured production capacity:

```text
min_connections: 1
max_connections: 10
acquire_timeout: 5 seconds
```

The pool validates that `max_connections` is positive, `min_connections` does not exceed it, and the acquire timeout is positive. Invalid configuration fails before attempting a database connection.

## Connection behavior

- Runtime connection uses the required `DATABASE_URL`; there is no SQLite/in-memory fallback.
- The connection URL is parsed before pool creation and malformed URLs produce a typed configuration error.
- Pool acquisition/connect failures are returned to the application and are not swallowed.
- Each acquired PostgreSQL session executes `SET TIME ZONE 'UTC'` through the pool’s connection hook, enforcing ARC-004 at the session boundary.
- Pool retry policy is not a hidden infinite loop; command retry/idempotency remains governed by ARC-008.
- Pool shutdown uses the pool’s explicit close behavior during graceful process shutdown.

The pool configuration uses `sqlx::postgres::PgPoolOptions`; its acquisition timeout covers waiting for a pool slot and opening a connection. The dependency is limited to the PostgreSQL driver and Tokio/Rustls runtime features.

## Ownership and use

- `football-infrastructure` owns the concrete `PgPool` construction.
- Application/domain code receives a port or repository abstraction rather than importing SQLx types into domain rules.
- Handlers do not create one pool per request.
- A pool is shared by request/worker tasks through its cloneable handle.

## Verification

Unit tests cover invalid pool configuration without a database. An ignored integration probe connects only when an explicit `TEST_DATABASE_URL` is supplied and verifies the session timezone. No test silently targets the developer’s default or a shared environment.

The implementation was checked against the current `sqlx` PostgreSQL pool API documented by [PgPoolOptions](https://docs.rs/sqlx/latest/sqlx/postgres/type.PgPoolOptions.html) and [PgConnectOptions](https://docs.rs/sqlx/latest/sqlx/postgres/struct.PgConnectOptions.html).

## Acceptance criteria

DB-002 is complete when the first-slice process has one validated shared PostgreSQL pool, bounded acquisition behavior, UTC sessions, typed connection errors, and database-free unit coverage for invalid configuration.

