# RT-001 — Realtime topic names

**Status:** Implemented; typed match topic naming contract and unit verification passed  
**Decision date:** 2026-08-09  
**Depends on:** [ARC-006 API versioning rules](ARC-006-api-versioning.md), [ARC-009 projection boundaries](ARC-009-projection-boundaries.md)

## Decision

At the time of this decision, realtime protocol major version `1` exposed one
topic scope:

```text
match:{match_id}
```

The topic identifies delivery scope for one match. It does not contain a
sequence number, authorization decision, notification payload, or canonical
state. Those remain separate contracts owned by RT-002 through RT-005 and the
REST snapshot boundary.

No competition-wide, standings, or global topic is introduced in this
increment because the repository has not yet defined their retention,
authorization, or synchronization semantics. New scopes require an explicit
topic contract before they are exposed to clients. The later standings scope
is introduced by [RT-016](RT-016-standings-revision-notifications.md).

## Implementation

`football-contracts::RealtimeTopic` owns the typed vocabulary and formats the
match identifier using the stable lowercase UUID representation. The realtime
protocol major is independent from REST API and canonical event schema
versions.

## Verification

The `football-contracts` unit test verifies protocol major `1`, the `match`
namespace, round-trip access to the match UUID, and the exact formatted topic
name.
