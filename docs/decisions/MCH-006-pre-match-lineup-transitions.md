# MCH-006 — Pre-match and lineup-confirmed transitions

**Status:** Accepted  
**Decision date:** 2026-08-08  
**Depends on:** [MCH-002 initial match states](MCH-002-initial-match-states.md), [MCH-003 match aggregate boundary](MCH-003-match-aggregate-boundary.md), [LIN-003 draft lineup persistence](LIN-003-draft-lineup-persistence.md)

## Decision

The aggregate exposes named domain operations for the first pre-match path:

```text
schedule
  → enter_pre_match
  → submit_lineups
  → confirm_lineups
```

These operations are the only public aggregate entry points for this
increment. They delegate to the existing typed lifecycle rules and therefore
reject skipped or out-of-order transitions. The aggregate does not expose its
mutable `MatchLifecycle` representation.

`submit_lineups` and `confirm_lineups` currently advance lifecycle state only.
The submitted lineup contents, team-side validation, player eligibility, and
lineup persistence remain owned by the lineup/application increments. The
application command must perform those validations before invoking the
corresponding aggregate operation; this increment does not invent a lineup
snapshot or mark an absent lineup as valid.

These lifecycle operations do not advance `AggregateVersion` yet. Version
advancement is reserved for accepted canonical event emission, which will be
implemented with command execution and event persistence increments.

## Verification

Database-free aggregate tests run the named path from `Draft` through
`LineupsConfirmed`, verify the final state, verify the version remains at its
initial value, and reject direct confirmation from `PreMatch`.
