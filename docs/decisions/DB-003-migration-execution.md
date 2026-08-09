# DB-003 — Migration naming and execution

**Status:** Accepted  
**Decision date:** 2026-08-08  
**Depends on:** [DB-002 PostgreSQL connection pool](DB-002-connection-pool.md), [ARC-007 forward-only migration policy](ARC-007-forward-only-migrations.md)

## Naming contract

Checked-in PostgreSQL migrations live in the repository-root `migrations/`
directory and use the SQLx-compatible filename format:

```text
<VERSION>_<DESCRIPTION>.sql
```

`VERSION` is a unique positive integer and `DESCRIPTION` is a stable lowercase
underscore-separated description. Applied migration files are immutable and
are never deleted or reused. There are no routine down-migration files.

The first business schema migration is intentionally deferred until its domain
slice defines the required tables and invariants. This increment does not add
speculative tables or business data.

## Runner contract

`football-infrastructure` exposes two explicit operations:

- `load_migrator(source)` resolves and validates the ordered migration set;
- `run_migrations(pool, source)` applies that set to an existing PostgreSQL
  pool.

The caller supplies the migration directory, so deployment and local tooling
can choose the intended checkout explicitly. The runner is not called while a
pool is created, from an HTTP handler, or as an implicit side effect of a
request.

SQLx maintains the `_sqlx_migrations` history table and verifies checksums for
already-applied files. A changed applied file therefore fails instead of
silently changing schema history. Migrations run transactionally by default;
an exception must be documented in the migration and reviewed under ARC-007.

## Safety boundary

DB-003 adds only the naming contract, source directory, runner, and database-free
resolution coverage. No real PostgreSQL migration is applied by this increment.
Clean-database application and constraint verification are the separate DB-004
increment and require an explicitly approved database target when executed.

## Acceptance evidence

- `migrations/README.md` records the filename and immutability rules.
- `football-infrastructure::load_migrator` resolves the repository directory in
  a Tokio test without contacting PostgreSQL.
- `football-infrastructure::run_migrations` compiles against the configured
  PostgreSQL pool and SQLx migration API.
