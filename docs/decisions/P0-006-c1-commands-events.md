# P0-006 — Initial C1 commands and canonical events

**Status:** Accepted for C1 implementation  
**Decision date:** 2026-08-08  
**Depends on:** [P0-005 canonical football vocabulary](P0-005-domain-vocabulary.md)

This record freezes the first C1 command vocabulary and the canonical events that represent accepted match behavior. Transport names and payload schemas may receive versioned contract details during the architecture and API increments, but the business meanings below must remain stable.

## Command contract

Every command is one idempotent business intention. It carries an authenticated actor, request ID, client command ID, request fingerprint, and the applicable expected aggregate version.

| Area | Commands |
|---|---|
| Scheduling and lineups | `ScheduleMatch`, `SubmitLineup`, `ConfirmLineup` |
| Match lifecycle | `StartFirstHalf`, `EndFirstHalf`, `StartSecondHalf`, `EndMatch` |
| Clock and interruption | `DeclareAddedTime`, `PauseClock`, `ResumeClock`, `CorrectClock`, `InterruptMatch`, `ResumeInterruptedMatch` |
| C1 live actions | `RecordGoal`, `RecordYellowCard`, `RecordSecondYellowCard`, `RecordRedCard`, `RecordSubstitution` |
| Correction and verification | `CorrectEvent`, `VoidEvent`, `VerifyMatch`, `ReopenMatch` |

## Command-to-event mapping

Each accepted command emits the corresponding canonical event below. A command may emit more than one event only when a domain invariant explicitly requires it; no extra event may be emitted merely to create a convenient statistic.

| Command | Canonical event |
|---|---|
| `ScheduleMatch` | `MatchScheduled` |
| `SubmitLineup` | `LineupSubmitted` |
| `ConfirmLineup` | `LineupConfirmed` |
| `StartFirstHalf` | `FirstHalfStarted` |
| `EndFirstHalf` | `FirstHalfEnded` |
| `StartSecondHalf` | `SecondHalfStarted` |
| `EndMatch` | `MatchFinished` |
| `DeclareAddedTime` | `AddedTimeDeclared` |
| `PauseClock` | `ClockPaused` |
| `ResumeClock` | `ClockResumed` |
| `CorrectClock` | `ClockCorrected` |
| `InterruptMatch` | `MatchInterrupted` |
| `ResumeInterruptedMatch` | `MatchResumed` |
| `RecordGoal` | `GoalRecorded` |
| `RecordYellowCard` | `YellowCardRecorded` |
| `RecordSecondYellowCard` | `SecondYellowCardRecorded` |
| `RecordRedCard` | `RedCardRecorded` |
| `RecordSubstitution` | `SubstitutionRecorded` |
| `CorrectEvent` | `EventCorrected` |
| `VoidEvent` | `EventVoided` |
| `VerifyMatch` | `MatchVerified` |
| `ReopenMatch` | `MatchReopened` |

## C1 event semantics

- `GoalRecorded` contains an award type: `Regular`, `Penalty`, or `OwnGoal`. These are not separate source events. An own goal increments the opponent’s score; a penalty goal increments the normal scoring side.
- `SecondYellowCardRecorded` is one source action. Its reduction creates the player’s sending-off state; a separate synthetic red-card event is not emitted for the same action.
- `RedCardRecorded` represents a direct red-card action.
- `SubstitutionRecorded` changes the players-on-the-pitch reduction and preserves the substitution history.
- `AddedTimeDeclared` records the declared additional time; it does not itself advance the clock every second.
- `MatchInterrupted` is an audited lifecycle decision with a reason. `PauseClock` is a timing action and is not a substitute for interruption.
- `EventCorrected` and `EventVoided` reference the affected immutable event and carry a reason. They never update or delete the original canonical event.
- `MatchVerified` records supervisor verification against the final authority. `MatchReopened` records an explicitly authorized reasoned reopening.

## Canonical event envelope

Every persisted C1 event includes the envelope fields defined by the roadmap:

```text
event_id
match_id
aggregate_version
causation_command_id
event_index
event_type
event_schema_version
period
clock_ms
occurred_at
recorded_at
actor_id
verification_status
payload
supersedes_event_id
voids_event_id
```

The event stream is append-only. For a command that emits `N` events at current aggregate version `v`, event versions are assigned `v+1` through `v+N` in deterministic order. `event_index` starts at zero for each command. Event payloads are validated before replay.

## C1 boundary

The initial C1 event set does not include shots, passes, duels, dribbles, touches, pressures, coordinates, xG, ratings, or other C2/C3 analytics events. A future action must represent one real football action as one canonical source event; derived counters are projections or attributes, not duplicate source events.

## Acceptance criteria

P0-006 is complete when the first slice can select a command from this vocabulary, produce only the mapped canonical event meaning, preserve idempotency and aggregate-version semantics, and correct/void prior events without mutating their original records.

