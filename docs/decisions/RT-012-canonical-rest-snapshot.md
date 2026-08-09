# RT-012 — Canonical REST snapshot integration

**Status:** Implemented; projection-to-contract mapping and snapshot envelope verification passed  
**Decision date:** 2026-08-09  
**Depends on:** [ARC-006 API versioning rules](ARC-006-api-versioning.md), [ARC-009 projection boundaries](ARC-009-projection-boundaries.md), [PRJ-001 synchronous score and timeline reduction](PRJ-001-synchronous-score-timeline-reduction.md), [RT-011 snapshot-required response](RT-011-snapshot-required-response.md)

## Decision

The canonical REST recovery resource is represented by the versioned
`MatchSnapshotResponse` contract. Its `data` is built only from the canonical
synchronous `MatchProjection`:

- match ID and home/away team IDs;
- score;
- canonical aggregate version; and
- ordered timeline entries with event identity, event version/index, event
  type, period, and football clock.

The response metadata carries contract version, an adapter-supplied UTC
`generated_at`, explicit `provisional`/`verified` status, and quality flags.
Blank metadata is rejected. No persistence row is exposed as a public DTO.

This increment deliberately does not invent match-level lifecycle or clock
state: those values are not currently persisted as a match snapshot and are
therefore not emitted by this minimal canonical C1 read contract. It also does
not infer verification status from missing data; the query adapter supplies it
explicitly.

`snapshot_required` remains a recovery signal. The public HTTP route that
serves this contract belongs to `PUB-001`; RT-012 freezes the canonical data
shape and the application mapping that route must use. No WebSocket handler or
database write is added here.

## Implementation

`football-contracts::MatchSnapshotResponse` owns the REST envelope and
`football-application::build_canonical_match_snapshot` maps a domain
`MatchProjection` into it. The mapping preserves canonical IDs, aggregate
version, timeline ordering, event identity, period, and clock values without
accepting operator totals or SQL rows.

## Verification

Contracts tests verify exact JSON output, metadata version/status/quality flags,
and rejection of blank metadata. Application tests verify an empty canonical
projection maps to a zero-score, version-zero snapshot with an empty timeline
and no fabricated lifecycle state.
