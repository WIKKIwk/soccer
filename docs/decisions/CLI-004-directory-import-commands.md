# CLI-004 — Team and player import commands

**Status:** Implemented; explicit team/player imports verified
**Decision date:** 2026-08-09
**Depends on:** [DIR-002 minimal team persistence](DIR-002-team-persistence.md), [DIR-004 minimal player-to-team persistence](DIR-004-player-team-persistence.md), and [CLI-003 competition and season setup commands](CLI-003-competition-season-commands.md)

## Decision

`footballctl` adds explicit directory import command names while preserving
the existing seed commands for compatibility:

```text
ALLOW_DEVELOPMENT_DIRECTORY_SEED=1 \
  DATABASE_URL=<approved-target> \
  cargo run -p footballctl -- directory import-teams \
    --first-id <uuidv7> --first-name <name> \
    --second-id <uuidv7> --second-name <name>

ALLOW_DEVELOPMENT_DIRECTORY_SEED=1 \
  DATABASE_URL=<approved-target> \
  cargo run -p footballctl -- directory import-player \
    --person-id <uuidv7> --player-id <uuidv7> \
    --team-id <uuidv7> --name <source-backed-name>
```

These are explicit source-backed directory writes, not a generic file-import
pipeline. CSV upload, staging, normalization, previews, validation reports,
and atomic approval remain the later `IMP-*` increments.

## Invariants

- Team and player IDs must be explicit UUIDv7 values.
- Team names and person names must contain non-whitespace source text.
- A player import requires an existing team.
- Repeating the exact import is idempotent.
- Reusing an identity with different data is rejected; existing rows are not
  silently renamed, moved, or overwritten.
- The existing `seed-teams` and `seed-player` command names continue to use
  the same parser and repository contract.

## Implementation

`apps/footballctl/src/main.rs` accepts `directory import-teams` and
`directory import-player` as aliases over the existing transactional directory
persistence paths. No new database tables or alternate identity source were
introduced.

`apps/footballctl/tests/directory_import.rs` exercises both imports, exact
repetition, read-back, a conflicting team name, and an unknown player team
against a clean PostgreSQL target.

## Verification

The ignored CLI integration test passed against a disposable
`football_socket_cli004` database after applying all repository migrations. The
disposable database was dropped; the persistent project test database was not
changed.
