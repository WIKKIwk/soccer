# DB-004 — Clean-database migration verification

**Status:** Implemented and clean-database verified on 2026-08-09  
**Decision date:** 2026-08-08  
**Depends on:** [DB-003 migration naming and execution](DB-003-migration-execution.md)

## Verification contract

`football-infrastructure/tests/clean_database_migrations.rs` is the explicit
PostgreSQL integration verification for the repository migration set. It:

1. reads only `TEST_DATABASE_URL`;
2. connects through the shared `connect_database` pool contract;
3. fails before mutation if `_sqlx_migrations` already exists;
4. resolves the checked-in `migrations/` directory;
5. applies the complete migration set through `run_migrations`; and
6. compares the applied migration versions with the resolved ordered versions.

The test is ignored by default. It must target a disposable, clean database and
be run only as an explicitly approved database operation. A second environment
flag is required so that supplying a URL alone cannot mutate a shared database:

```text
ALLOW_CLEAN_DATABASE_MIGRATION=1 \
TEST_DATABASE_URL='<approved-clean-database-url>' \
  cargo test -p football-infrastructure \
  --test clean_database_migrations -- --ignored --nocapture
```

The test does not silently select a local default database and does not create,
drop, or clean a database. A successful run leaves SQLx migration history in
the target; therefore the target must be disposable or reserved for this
verification.

## Current status

The fixture ran successfully with
`ALLOW_CLEAN_DATABASE_MIGRATION=1` against the dedicated
`football_socket_test_migrations` database and applied all seven repository
migrations. The same clean-database contract was then exercised by the
command, event, C1, concurrency, and correction integration fixtures against
separate dedicated project test databases. Those temporary databases were
removed after their tests; the persistent `football_socket_test` database is
reserved for this project. Existing databases were not modified.
