# CLI-003 — Competition and season setup commands

**Status:** Implemented; minimal competition/season persistence and CLI seed flow verified
**Decision date:** 2026-08-09
**Depends on:** [CMP-001 minimal competition and season](CMP-001-minimal-competition-season.md) and [ARC-007 forward-only migration policy](ARC-007-forward-only-migrations.md)

## Decision

`footballctl` exposes two explicit setup commands for the minimal competition
vertical slice:

```text
ALLOW_DEVELOPMENT_COMPETITION_SEED=1 \
  DATABASE_URL=<approved-target> \
  cargo run -p footballctl -- competition seed \
    --id <uuidv7> --name <source-backed-name>

ALLOW_DEVELOPMENT_COMPETITION_SEED=1 \
  DATABASE_URL=<approved-target> \
  cargo run -p footballctl -- season seed \
    --id <uuidv7> --competition-id <uuidv7> --year <positive-integer>
```

The commands accept only explicit UUIDv7 identities and source-backed values.
They do not invent names, years, statuses, types, dates, stages, or rules.
They never run migrations implicitly. Persistence requires the dedicated
development authorization environment variable and reads the record back
before reporting success.

## Persistence invariants

- A competition stores its typed ID and required source name.
- A season stores its typed ID, owning competition ID, and positive year.
- A season cannot be persisted without an existing competition.
- One competition cannot have two seasons for the same year.
- Repeating the exact competition or season command is idempotent.
- Reusing an existing identity with different data is rejected.
- Existing fixture rows retain their canonical IDs; foreign-key expansion from
  fixtures to these master tables remains a later compatibility increment.

## Implementation

Migration `0016_competitions_seasons.sql` adds the minimal `competitions` and
`seasons` tables with non-blank, positive-year, ownership, and uniqueness
constraints. `crates/infrastructure/src/competition.rs` owns the typed row
conversion, persistence, read-back, idempotency, and conflict behavior.

`apps/footballctl/src/main.rs` adds the `competition seed` and `season seed`
adapters while preserving the existing explicit authorization and no-implicit-
migration conventions.

Competition lifecycle/status, season date intervals, stages, groups, rounds,
and versioned rules remain outside this increment.

## Verification

The ignored CLI integration test passed against a disposable
`football_socket_cli003` database after applying every repository migration. It
executed both commands, repeated both exact commands, loaded the persisted
records through the infrastructure repository, and rejected a season whose
competition did not exist. The disposable database was dropped; the persistent
project test database was not changed.
