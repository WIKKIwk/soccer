# P0-004 — Minimum public mobile screens

**Status:** Accepted for the initial MVP  
**Decision date:** 2026-08-08  
**Depends on:** [P0-001 product contract](P0-001-product-contract.md)

This record defines the smallest public mobile information surface required to consume the C1 platform. It defines screen responsibilities, not visual design or Flutter implementation details.

## Initial screen set

| Screen | Required public information | Initial behavior |
|---|---|---|
| Competition season overview | Competition, selected season, latest/upcoming matches, standings entry point | Starts with O‘zbekiston Superligasi 2026; the model remains season-aware. |
| Match list | Fixtures and results by date, live-match filter, match status, provisional/verified indication | Read-only list with bounded pagination/filtering. |
| Match detail | Teams, score, lifecycle state, clock, verification state, quality flags, timeline | REST snapshot is sufficient for the first vertical slice. |
| Match lineups | Confirmed/provisional starters and substitutes for both teams | Read-only match-specific lineup view. |
| Standings | Played, won, drawn, lost, goals, points, position, revision freshness | Shows whether the projection is current or rebuilding. |

The lineup view may be implemented as a tab or subview of match detail, but its public contract remains explicit because lineup data has its own read model and acceptance behavior.

## Explicitly outside the minimum screen set

- public web client;
- operator or supervisor mutation screens;
- team-profile and player-profile mobile screens;
- advanced statistics, xG, ratings, heatmaps, pass maps, tracking, and C3 analytics;
- offline canonical mutation workflow.

Team and player master data remain part of the backend/domain scope. Their mobile presentation can be added after the core C1 mobile flow without changing canonical match behavior.

## Client and delivery rules

- Screens are public read clients and never write canonical match state.
- The first vertical slice uses REST snapshots only.
- WebSocket delivery is added after the minimal REST match flow, replay, and correction behavior are reliable.
- Verification status, quality flags, and standings freshness are visible metadata rather than hidden assumptions.
- `footballctl` remains the initial operator adapter; it is not a public mobile screen.

## Acceptance criteria

P0-004 is complete when the product and implementation can map the initial C1 public flow to these five information surfaces without requiring an extra public screen or a write-capable mobile client.

