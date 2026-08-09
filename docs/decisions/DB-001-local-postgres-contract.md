# DB-001 — Local PostgreSQL connection contract

**Status:** Accepted  
**Decision date:** 2026-08-08  
**Depends on:** [ARC-004 UTC/timezone rules](../decisions/ARC-004-utc-timezone-rules.md) and [ARC-007 migration policy](../decisions/ARC-007-forward-only-migrations.md)

## Verified local baseline

Read-only environment inspection on 2026-08-08 found:

```text
psql: PostgreSQL 17.10 (Homebrew)
host: 127.0.0.1
port: 5432
pg_isready: accepting connections
current database: postgres
```

The probe authenticated as the local `superuser` account. That account is evidence of the developer machine only; it is not the application-role contract and is not copied into configuration.

## Required runtime contract

The application receives one required connection URL:

```text
DATABASE_URL=postgres://<app_user>:<app_password>@127.0.0.1:5432/<football_database>?application_name=football-platform
```

PostgreSQL is the authoritative persistence system. The runtime must not silently fall back to SQLite, an in-memory store, or a different database when `DATABASE_URL` is missing or invalid.

The connection URL must identify a local development host for local work. Production/test deployment URLs are supplied by their approved environment and are not committed to the repository.

## Integration-test contract

PostgreSQL integration tests use a separate explicit URL:

```text
TEST_DATABASE_URL=postgres://<test_user>:<test_password>@127.0.0.1:5432/<football_test_database>?application_name=football-platform-tests
```

The test database must not be the developer’s default or a real shared environment. Destructive test setup is allowed only after the resolved URL is verified as the approved test target. Domain unit tests remain database-free.

## Role and privilege boundary

- The application uses a dedicated non-superuser role.
- Migration ownership/privileges are handled explicitly by the database setup task; the runtime role does not receive unrestricted server administration privileges.
- The local `superuser` account discovered during inspection is not placed in `.env.example`, source, logs, or committed configuration.
- Credentials are supplied through the environment or local secret store and never committed.

## Session and connection rules

- Database sessions use UTC as required by ARC-004.
- `application_name` identifies the platform or integration-test process.
- Connection pooling, maximum connections, timeouts, and retry behavior are defined in DB-002; this task does not invent pool settings.
- A connection failure is observable and blocks database-dependent readiness; it is not swallowed as a healthy state.

## Repository contract

`.env.example` documents variable names and URL shape only. Placeholder credentials and database names are not usable secrets. A real `.env` file is local-only and must be ignored before source-control initialization.

No database, role, schema, migration, or real data was created or changed by DB-001.

## Acceptance criteria

DB-001 is complete when the runtime and integration-test URLs, local host/port, role boundary, UTC session rule, secret handling, and no-fallback behavior are explicit and the local PostgreSQL baseline has been probed read-only.

