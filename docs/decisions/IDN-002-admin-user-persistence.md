# IDN-002 — Administrator persistence and controlled development seed

**Status:** Accepted  
**Decision date:** 2026-08-08  
**Depends on:** [IDN-001 administrator ID and actor identity](IDN-001-admin-user-actor-identity.md), [ARC-007 forward-only migrations](ARC-007-forward-only-migrations.md), [DB-004 clean database verification](DB-004-clean-database-verification.md)

## Decision

Administrator identity persistence starts with the forward-only migration
`migrations/0001_admin_users.sql`. The `admin_users` table stores:

- canonical UUID primary key;
- unique username;
- display name;
- explicit active flag;
- creation and update timestamps.

The schema does not contain passwords, session secrets, roles, or permissions.
Those concerns belong to later identity increments. The database uses native
`uuid` storage; the application boundary supplies the UUIDv7-backed
`AdminUserId`.

## Controlled development seed boundary

The migration creates schema only. It does not insert a guessed administrator
or a production-wide development account. Creating one active development
administrator is an explicit operation that must receive all of:

1. an approved controlled identity (`AdminUserId`, username, and display name);
2. an explicitly approved disposable or local database target; and
3. a verified insert/read-back check through the infrastructure/application
   boundary.

The repository exposes that operation through the development-only command:

```text
ALLOW_DEVELOPMENT_ADMIN_SEED=1 \
  DATABASE_URL=<approved-target> \
  cargo run -p footballctl -- admin seed-development \
    --id <uuidv7> \
    --username <username> \
    --display-name <display-name>
```

The command does not run migrations implicitly. It rejects non-UUIDv7 IDs,
requires explicit authorization, refuses conflicting existing records, and
reads the inserted row back before reporting success.

Process startup, health requests, and ordinary HTTP requests must not create an
administrator implicitly. The real employee identity remains unresolved as
recorded by P0-001.

## Verification status

The migration is database-free parseable and included in the repository’s
ordered migration set. The command parser, input validation, and read-back
logic have database-free tests. IDN-002 remains incomplete until an approved
identity and clean target are supplied for the explicit seed and read-back
verification.
