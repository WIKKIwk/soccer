# P0-001 — Product contract and MVP match flow

**Status:** Accepted  
**Decision date:** 2026-08-08  
**Coverage:** C1

This decision record establishes the first competition, season, data-authority model, operational roles, public-client boundary, and initial locale policy. It is the product contract for the first vertical slice.

## Decision

```yaml
decision_id: P0-001
status: accepted
decided_at: 2026-08-08

competition:
  name: "O‘zbekiston Superligasi"
  season: "2026"
  coverage: "C1"

data_authority:
  live_source: "assigned_operator_observation"
  live_status: "provisional"
  permitted_observation_channels:
    - official_live_broadcast
    - on_site_observation

  final_sources:
    - official_pfl_match_protocol
    - official_pfl_result
    - official_pfl_disciplinary_decision

  conflict_resolution:
    actor: match_data_supervisor
    method:
      - correct_event
      - void_event
      - verify_match

operations:
  administrator_role: administrator
  actual_administrator: null
  administrator_responsibilities:
    - configure_competition_and_season_data
    - manage_directory_and_fixture_setup
    - manage_access_and_assignments
  operator_role: match_data_operator
  primary_operators_per_match: 1
  actual_operator: null
  operator_required_before: live_pilot

  supervisor_role: match_data_supervisor
  actual_supervisor: null
  supervisor_required_before: live_pilot
  supervisor_must_be_distinct_from_operator: true

public_clients:
  primary:
    - android
    - ios
  development:
    - footballctl
    - rest_api_test_client
  excluded_from_initial_mvp:
    - public_web
  realtime_sequence: "after_minimal_rest_vertical_slice"

localization:
  initial_content_locales:
    - uz-Latn
  schema_supported_locales:
    - uz-Latn
    - uz-Cyrl
    - ru
    - en
  deferred_content_locales:
    - uz-Cyrl
    - ru
    - en
```

## Authority and correction policy

The `Match Data Operator` is a collector, not an authoritative source. Operator observations are accepted as provisional live data only.

The final verified state is based on the official Professional Football League of Uzbekistan (PFL) match protocol, official result, and official disciplinary decisions. When provisional operator data conflicts with an official record, a `Match Data Supervisor` applies a reasoned `correct_event` or `void_event` operation and then verifies the match. Published canonical events remain immutable; the correction is represented by new canonical events and deterministic replay.

The PFL source used for this decision states that the 2026 Uzbekistan Super League season started on 27 February 2026: [PFL.UZ — Superliga. 1-turning markaziy o‘yiniga marhamat](https://pfl.uz/news/superliga-1-turning-markaziy-oyiniga-marhamat).

## Roles

The operator and supervisor are separate responsibilities:

- `Administrator` — configures competition, season, directory, fixture, access, and assignment data through the admin boundary.
- `Match Data Operator` — enters live match observations for one assigned match. One primary operator is assigned per match.
- `Match Data Supervisor` — reviews final state, approves critical corrections, handles reopen, and verifies the result.

The actual employee identity records are intentionally unresolved until the pilot staffing decision. This does not block domain roles, authorization boundaries, or application contracts. The real operator and supervisor must be assigned before live pilot operation.

## Initial acceptance flow

```text
create competition
→ create season
→ register two teams
→ register players
→ schedule fixture
→ assign operator
→ submit and confirm lineups
→ start first half
→ record goal/card/substitution
→ declare added time if required
→ interrupt/resume if required
→ finish match
→ correct or void an event
→ verify result
→ update standings revision
→ deliver public REST state
→ add WebSocket delivery after the minimal REST slice is reliable
```

## First-release exclusions

The first release is C1. Public web is outside the initial MVP client boundary. The following remain outside the mandatory first-year core until C1 succeeds in real pilot matches: xG, player ratings, heatmaps, pass maps, player tracking, full C3 event capture, machine-learning models, recommendation systems, speculative search infrastructure, premature microservices, NATS, Redis/Dragonfly, Kubernetes, and multiple authoritative databases.

## Remaining Week 1 decisions

This record completes the product authority, role, client, locale, and first-flow decisions supplied for P0-001. The following roadmap items still require their own evidence before the Week 1 gate can pass:

- `P0-004` — minimum public mobile screens;
- `P0-005` — confirmed domain vocabulary;
- `P0-006` — explicitly confirmed initial C1 command/event list.
