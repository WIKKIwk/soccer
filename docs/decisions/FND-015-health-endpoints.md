# FND-015 — Liveness and database readiness

**Status:** Accepted  
**Decision date:** 2026-08-08  
**Depends on:** [DB-002 PostgreSQL connection pool](DB-002-connection-pool.md), [FND-008 typed configuration](FND-008-typed-configuration.md), [FND-014 request IDs](FND-014-request-ids.md)

## Decision

The admin API exposes two operational endpoints:

- `GET /healthz` — liveness. It returns `200` with `{"status":"ok"}` when
  the process can serve HTTP. It does not access PostgreSQL.
- `GET /readyz` — database readiness. It runs `SELECT 1` through the shared
  PostgreSQL pool and returns `200` with `{"status":"ready"}` only when the
  dependency check succeeds. A failed check returns `503` with the public
  body `{"status":"not_ready"}`.

The readiness response never exposes connection strings, SQLx errors, host
names, credentials, or other dependency details. The failure is recorded as a
structured warning with only the dependency name. Request-ID middleware still
adds the correlation header and log field to both endpoints.

The admin API creates a validated lazy PostgreSQL pool at startup. This parses
and validates `DATABASE_URL` without requiring an immediate connection, so a
database outage makes readiness fail while liveness remains available. Actual
database operations still receive the connection failure through the shared
pool contract.

## Boundary

The database pool and `SELECT 1` probe belong to
`football-infrastructure`. The route and public response belong to the admin
HTTP adapter. Health checks do not run migrations, create data, or expose
dependency internals.

## Verification

Admin API tests verify that liveness works without a database and that an
unreachable database produces a public `503` without dependency details. A
local process smoke test must verify `/healthz` and `/readyz` separately with
the request ID header present on both responses.
