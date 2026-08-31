mod actions;
mod aggregate;
mod canonical_payload;
mod clock;
mod corrections;
mod effective_stream;
mod envelopes;
mod events;
mod identity;
mod lifecycle_commands;
mod lifecycle_events;
mod lifecycle_projection;
mod projection;
mod schema;
mod state;
mod stream;
mod verification;
mod verification_events;
mod version;

pub use actions::{C1LiveAction, C1LiveActionCommand, C1LiveActionCommandError};
pub use aggregate::MatchAggregate;
pub use canonical_payload::MatchEventPayload;
pub use clock::{
    ClockError, ClockMilliseconds, ClockMillisecondsError, ClockRevision, ClockRevisionError,
    MatchClock, MatchPeriod,
};
pub use corrections::{
    CorrectionEventPayload, CorrectionEventType, CorrectionReason, CorrectionReasonError,
    EventVoidedPayload, VoidEventCommand, VoidEventCommandError,
};
pub use effective_stream::{
    CorrectionReferenceKind, EffectiveEventStream, EffectiveEventStreamError,
};
pub use envelopes::{
    CommandId, CommandOutcome, CommandResultEnvelope, CommandResultStatus, DomainEventEnvelope,
    EventId, EventIndex, EventIndexError, EventSchemaVersion, EventSchemaVersionError, EventType,
    EventTypeError, EventVerificationStatus,
};
pub use events::{
    C1EventPayload, C1EventType, GoalAwardType, GoalRecordedPayload, RedCardRecordedPayload,
    SecondYellowCardRecordedPayload, SubstitutionPayloadError, SubstitutionRecordedPayload,
    YellowCardRecordedPayload,
};
pub use identity::{MatchId, MatchIdentity};
pub use lifecycle_commands::{MatchLifecycleCommand, MatchLifecycleCommandError};
pub use lifecycle_events::{LifecycleEventPayload, LifecycleEventType, LineupSubmittedPayload};
pub use lifecycle_projection::{LifecycleProjection, LifecycleProjectionError};
pub use projection::{MatchProjection, MatchProjectionError, MatchScore, TimelineEntry};
pub use schema::{
    EventSchemaDefinition, EventSchemaDefinitionError, EventSchemaKey, EventSchemaLookupError,
    EventSchemaRegistry, EventSchemaRegistryError, EventSchemaResolution,
};
pub use state::{MatchLifecycle, MatchState, MatchTransitionError};
pub use stream::{CanonicalEventStream, EventAppendError, EventStreamError};
pub use verification::{MatchVerificationStatus, MatchVerificationTransitionError};
pub use verification_events::{
    MatchReopenedPayload, MatchVerifiedPayload, ReopenMatchCommand, ReopenMatchCommandError,
    VerificationEventPayload, VerificationEventType, VerifyMatchCommand, VerifyMatchCommandError,
};
pub use version::{AggregateVersion, AggregateVersionError};
