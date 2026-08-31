#![forbid(unsafe_code)]

//! Football domain rules, aggregates, value objects, and canonical events.

pub mod command;
pub mod competition;
pub mod directory;
pub mod evidence;
pub mod identity;
pub mod lineups;
pub mod match_engine;
pub mod operations;
pub mod people;
pub mod scheduling;
pub mod standings;

pub use command::{
    ClientCommandId, ClientCommandIdError, CommandStatus, CommandStatusTransitionError,
    RequestFingerprint,
};
pub use competition::{
    Competition, CompetitionId, CompetitionName, CompetitionNameError, Season, SeasonId,
    SeasonResultsRevision, SeasonResultsRevisionError, SeasonYear, SeasonYearError,
};
pub use directory::{Team, TeamId, TeamName, TeamNameError};
pub use evidence::{EvidenceReference, EvidenceReferenceError};
pub use identity::{ActorIdentity, AdminUserId};
pub use lineups::{
    DraftLineup, LineupEntry, LineupId, LineupRole, LineupValidationError, validate_unique_players,
};
pub use match_engine::{
    AggregateVersion, AggregateVersionError, C1EventPayload, C1EventType, C1LiveAction,
    C1LiveActionCommand, C1LiveActionCommandError, CanonicalEventStream, ClockError,
    ClockMilliseconds, ClockMillisecondsError, ClockRevision, ClockRevisionError, CommandId,
    CommandOutcome, CommandResultEnvelope, CommandResultStatus, CorrectionEventPayload,
    CorrectionEventType, CorrectionReason, CorrectionReasonError, CorrectionReferenceKind,
    DomainEventEnvelope, EffectiveEventStream, EffectiveEventStreamError, EventAppendError,
    EventId, EventIndex, EventIndexError, EventSchemaDefinition, EventSchemaDefinitionError,
    EventSchemaKey, EventSchemaLookupError, EventSchemaRegistry, EventSchemaRegistryError,
    EventSchemaResolution, EventSchemaVersion, EventSchemaVersionError, EventStreamError,
    EventType, EventTypeError, EventVerificationStatus, EventVoidedPayload, GoalAwardType,
    GoalRecordedPayload, LifecycleEventPayload, LifecycleEventType, LifecycleProjection,
    LifecycleProjectionError, LineupSubmittedPayload, MatchAggregate, MatchClock,
    MatchEventPayload, MatchId, MatchIdentity, MatchLifecycle, MatchLifecycleCommand,
    MatchLifecycleCommandError, MatchPeriod, MatchProjection, MatchProjectionError,
    MatchReopenedPayload, MatchScore, MatchState, MatchTransitionError, MatchVerificationStatus,
    MatchVerificationTransitionError, MatchVerifiedPayload, RedCardRecordedPayload,
    ReopenMatchCommand, ReopenMatchCommandError, SecondYellowCardRecordedPayload,
    SubstitutionPayloadError, SubstitutionRecordedPayload, TimelineEntry, VerificationEventPayload,
    VerificationEventType, VerifyMatchCommand, VerifyMatchCommandError, VoidEventCommand,
    VoidEventCommandError, YellowCardRecordedPayload,
};
pub use operations::{
    MatchAssignment, OperatorLease, OperatorLeaseError, OperatorLeaseExpiryPolicy,
    OperatorLeaseExpiryPolicyError, OperatorLeaseId,
};
pub use people::{
    Person, PersonId, PersonName, PersonNameError, Player, PlayerId, TeamPlayerAssignment,
};
pub use scheduling::{Fixture, FixtureError, FixtureId, UtcInstant};
pub use standings::{
    MatchGoalsPointsReduction, MatchOutcome, MatchOutcomeReduction, PointsRule,
    StandingComparisonRow, TeamGoalsPointsDelta, TeamOutcomeDelta, TiebreakCriterion,
    TiebreakDirection, TiebreakMetric, TiebreakPolicy, TiebreakPolicyError, VerifiedMatchResult,
    VerifiedMatchResultError, compare_standing_rows, reduce_goals_and_points,
    reduce_played_won_drawn_lost,
};
