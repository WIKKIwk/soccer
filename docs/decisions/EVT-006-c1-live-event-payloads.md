# EVT-006 — C1 live event payloads

**Status:** Implemented as a typed domain and infrastructure codec boundary; database round-trip verified on 2026-08-09  
**Decision date:** 2026-08-09  
**Depends on:** [P0-006 initial C1 commands and events](P0-006-c1-commands-events.md), [P0-005 canonical football vocabulary](P0-005-domain-vocabulary.md), and [EVT-002 event schema versioning](EVT-002-event-schema-versioning.md)

## Decision

The first C1 live-action payload set is typed in the match domain:

- `GoalRecordedPayload` with the credited team, optional known scorer, and
  `GoalAwardType::{Regular, Penalty, OwnGoal}`;
- separate player payloads for `YellowCardRecorded`,
  `SecondYellowCardRecorded`, and `RedCardRecorded`; and
- `SubstitutionRecordedPayload` with team, player-on, and player-off IDs.

`C1EventType` freezes the five canonical event names and schema version `1`.
`C1EventPayload` keeps the event meaning associated with its payload instead of
allowing a card or action to be silently relabeled as another event.

The goal's `team_id` is the side credited with the score. `scorer_player_id`
is optional because a provisional observation may not yet identify the player;
the domain does not fabricate a placeholder identity. For an own goal, the
optional player is the player whose own goal produced the credited team's
score. Score reduction and lineup/players-on-pitch eligibility checks remain
aggregate concerns and are not guessed inside these payload constructors.

`SecondYellowCardRecorded` is intentionally distinct from `RedCardRecorded`.
The former is one source action whose reduction later creates the sending-off
state; no synthetic direct-red event is introduced. A substitution cannot name
the same player as both entering and leaving.

## Command-to-event mapping

`C1LiveAction` is the domain vocabulary for the five first-release live
intentions: `RecordGoal`, `RecordYellowCard`, `RecordSecondYellowCard`,
`RecordRedCard`, and `RecordSubstitution`. Each action maps to exactly one
matching `C1EventPayload` variant and therefore to the corresponding canonical
`C1EventType`.

`C1LiveActionCommand` carries the command/event IDs, match identity, expected
aggregate version, period, clock, timestamps, actor, and verification status.
Construction accepts only a live match period; it does not fabricate an event
for half-time, full-time, interruption, or other break periods. Its conversion
creates an unsequenced `DomainEventEnvelope`: the canonical event stream later
assigns the actual aggregate version and per-command event index. When a
previously loaded stream is available, `append_to_stream` applies this command's
expected version through the generic stream boundary. Aggregate loading,
authorization, and persistence stay outside this domain operation.

The C1 projection repeats the live-period invariant during replay. A malformed
or manually inserted C1 envelope with `pre_match` or `half_time` is rejected
before score or timeline state changes; this protects replay even when the
event did not come through the command constructor.

## Persisted payload boundary

The infrastructure adapter persists the typed payloads as strict version-1 JSON
objects. Envelope metadata remains in typed PostgreSQL columns; the JSON body
contains only event-specific fields:

- `GoalRecorded`: `team_id`, nullable `scorer_player_id`, and `award_type`;
- each card event: `player_id`; and
- `SubstitutionRecorded`: `team_id`, `player_on_id`, and `player_off_id`.

UUID fields are stored as canonical strings and the goal award values are
`regular`, `penalty`, and `own_goal`. Unknown fields, missing fields, invalid
UUIDs, unsupported event types, unsupported schema versions, and invalid
substitution direction are rejected. The typed envelope codec also rejects
`pre_match` and `half_time` periods, so an invalid C1 event cannot enter the
persist path and is rejected again if malformed storage is encountered. The
typed loader returns
`CanonicalEventStream<C1EventPayload>` for domain replay; the generic loader
continues to preserve raw JSON for event types not yet implemented.

## Verification

Database-free domain and infrastructure tests cover stable event names and
schema versions, goal award/team/scorer preservation, distinct card meanings,
substitution direction, same-player substitution rejection, strict JSON
round-trips, envelope metadata preservation, live-period enforcement, and
malformed payload rejection. The PostgreSQL typed persist/load round-trip
passed for goal and yellow-card payloads in a fresh dedicated database. Unit
and domain fixtures cover the complete five-action C1 vocabulary, including
red, second-yellow, and substitution payloads. Default verification remains
database-free, while the approved integration run used only dedicated project
test databases.
