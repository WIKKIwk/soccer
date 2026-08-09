# DIR-002 — Minimal team persistence

**Status:** Accepted  
**Decision date:** 2026-08-08  
**Depends on:** [DIR-001 minimal team identity and name](DIR-001-team-identity.md), [ARC-007 forward-only migrations](ARC-007-forward-only-migrations.md)

## Decision

The first directory persistence migration creates a `teams` table with only
the canonical `TeamId`, `TeamName`, and creation/update timestamps. It does not
add club relationships, squad membership, competition registration, status,
localized names, or fixture ownership.

Two explicit teams can be persisted and read back through:

```text
ALLOW_DEVELOPMENT_DIRECTORY_SEED=1 \
  DATABASE_URL=<approved-target> \
  cargo run -p footballctl -- directory seed-teams \
    --first-id <uuidv7> --first-name <name> \
    --second-id <uuidv7> --second-name <name>
```

The command requires distinct UUIDv7 IDs and non-empty names. It refuses
conflicting existing IDs, reads each inserted record back, and never runs
migrations implicitly. Team names are supplied by the caller; the platform
does not invent competition data.

## Verification status

Domain and CLI validation are database-free tested. The migration and
repository are implemented, but DIR-002 remains incomplete until two
approved source-backed team records are inserted and read back on a clean
approved database target.
