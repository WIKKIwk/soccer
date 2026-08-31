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

Completed on 2026-08-14. Domain and CLI validation are database-free tested;
the migration and repository were also exercised against the dedicated local
database `football_socket_test_idn002`. The guarded `footballctl directory
seed-teams` command inserted and read back two explicit Superliga team records,
`Andijon` and `Buxoro`, using the PFL's 2026 Superliga match report as the
source-backed reference: [PFL.UZ — Andijon Buxoro’ni ishonchli hisobda yengdi](https://pfl.uz/news/superliga-andijon-buxoroni-ishonchli-hisobda-yengdi).

The focused `directory_import` PostgreSQL fixture also passed for the
two-team path, idempotent re-run, read-back, conflicting-name rejection, and
unknown-team rejection. The fixture's synthetic player setup was used only to
exercise the dependent path; DIR-004 remains a separate, open roadmap task.
