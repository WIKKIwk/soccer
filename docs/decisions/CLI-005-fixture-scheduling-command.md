# CLI-005 — Fixture scheduling command

**Status:** Implemented; fixture scheduling and UTC persistence verified
**Decision date:** 2026-08-09
**Depends on:** [SCH-001 minimal fixture](SCH-001-minimal-fixture.md), [CLI-003 competition and season commands](CLI-003-competition-season-commands.md), [CLI-004 directory import commands](CLI-004-directory-import-commands.md), and [ARC-004 UTC and timezone rules](ARC-004-utc-timezone-rules.md)

## Decision

`footballctl` exposes one minimal scheduling command:

```text
ALLOW_DEVELOPMENT_SCHEDULING_SEED=1 \
  DATABASE_URL=<approved-target> \
  cargo run -p footballctl -- fixture schedule \
    --id <uuidv7> \
    --season-id <uuidv7> \
    --home-team-id <uuidv7> \
    --away-team-id <uuidv7> \
    --kickoff 2026-08-15T19:00:00+05:00
```

The season supplies the competition ownership. The command requires an
existing season and existing home/away teams, rejects the same team on both
sides, accepts only an RFC3339 kickoff with an explicit offset, and stores the
canonical instant in UTC.

## Invariants

- Fixture and referenced IDs are explicit UUIDv7 values.
- The referenced season must exist.
- The season’s competition ID is the fixture’s competition ownership; it is
  not independently supplied or fabricated by the CLI.
- Both home and away teams must already exist.
- Home and away teams must be distinct.
- Repeating the exact fixture command is idempotent.
- Reusing a fixture ID with different data is rejected.
- No venue, stage, round, status, postponement, cancellation, or officials
  fields are inferred in this minimal slice.

## Implementation

`crates/infrastructure/src/fixtures.rs` owns fixture persistence and read-back.
It validates season/team ownership before insert, preserves fixture identity,
and fails closed on stored ownership mismatches. `apps/footballctl/src/main.rs`
parses the explicit-offset kickoff and constructs the domain `Fixture` from the
persisted season before calling the repository.

The existing `fixtures` table from migration `0004` is used. No implicit
migration or alternate fixture store is introduced.

## Verification

The ignored CLI integration test passed against a disposable
`football_socket_cli005` database after applying all repository migrations. It
verified UTC read-back, exact repetition, same-team rejection, unknown-team
rejection, and fixture-identity conflict. The disposable database was dropped;
the persistent project test database was not changed.
