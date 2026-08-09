# PostgreSQL migrations

Migration files in this directory are immutable, ordered, forward-only SQL
scripts. Use this filename format:

```text
<VERSION>_<DESCRIPTION>.sql
```

Examples:

```text
0001_initial_platform.sql
0002_match_events.sql
```

`VERSION` is a unique positive integer. `DESCRIPTION` is a short stable
description using lowercase words separated by underscores. Once a migration
has been applied, do not edit, delete, or reuse its filename or contents; add a
new migration for a forward fix. Routine down migrations are not supported.

The infrastructure migration runner resolves this directory and applies the
ordered set through SQLx. SQLx records applied migration versions and checksums
in `_sqlx_migrations`; a checksum mismatch stops execution. Migration execution
is an explicit operations step and is not performed during pool creation or an
HTTP request.
