# QUA-002: supervisor verification permissions

**Status:** Implemented
**Decision date:** 2026-08-09
**Depends on:** [P0-001 product contract](P0-001-product-contract.md), [QUA-001 provisional and verified match state](QUA-001-provisional-verified-state.md), and [RT-004 realtime topic authorization](RT-004-topic-authorization.md)

## Decision

`VerifyMatch` and `ReopenMatch` are supervisor-only operations. The
application authorization boundary receives two explicit inputs:

- an authenticated role claim, either `MatchDataOperator` or
  `MatchDataSupervisor`;
- operation-specific access facts, `can_verify` and `can_reopen`, supplied by
  the identity/access query boundary.

An operator is rejected for both operations even if capability facts are
incorrectly or overly broadly supplied. A supervisor is rejected when the
specific operation capability is absent. Missing capability facts therefore
fail closed, and granting `can_reopen` does not grant `can_verify` (or vice
versa).

The policy is transport-independent and has no side effects. It runs before a
future verification command can mutate the match aggregate or persist a
canonical `MatchVerified`/`MatchReopened` event. It does not infer a role or
scope from an administrator UUID.

## Scope boundary

This increment does not implement authentication, sessions, persistent RBAC,
role assignment, competition scope, verification commands, evidence, reasons,
audit, or canonical event producers. Persistent role and permission ownership
remains with `IDN-010` through `IDN-013`; command execution belongs to the
following QUA increments.

No database migration was added and no existing `admin_users` row shape was
repurposed.

## Implementation

`football-application::verification_authorization` owns the typed principal,
operation, capability facts, authorization errors, and fail-closed policy.
The boundary follows the same authoritative-facts approach as the realtime
topic authorization policy: adapters must load role and scope facts rather
than derive permission from a role name or ID alone.

## Verification

Application unit tests cover supervisor verify authorization, independent
verify/reopen capabilities, operator denial for both operations, stable
operation names, and fail-closed missing capability facts.
