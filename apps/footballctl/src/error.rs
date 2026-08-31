use crate::{auth, auth::AuthorizationScope};
use football_domain::{
    AdminUserId, CompetitionId, CompetitionNameError, FixtureError, FixtureId, MatchId,
    OperatorLeaseExpiryPolicyError, OperatorLeaseId, PersonNameError, PlayerId, SeasonId,
    SeasonYearError, TeamId, TeamNameError,
};
use football_infrastructure::{
    CompetitionRepositoryError, FixtureRepositoryError, OperatorAssignmentRepositoryError,
    PlayerRepositoryError, TeamRepositoryError,
};
use std::{error::Error, fmt};

#[derive(Debug)]
pub(crate) enum CliError {
    Usage,
    InvalidUuid,
    UuidMustBeV7,
    SeedNotAuthorized,
    DirectorySeedNotAuthorized,
    CompetitionSeedNotAuthorized,
    SchedulingSeedNotAuthorized,
    OperationsSeedNotAuthorized,
    InvalidCompetitionName(CompetitionNameError),
    InvalidSeasonYear(SeasonYearError),
    InvalidSeasonYearValue,
    InvalidKickoff(time::error::Parse),
    InvalidCurrentTime(time::error::ComponentRange),
    InvalidLeaseSeconds,
    InvalidLeaseExpiryPolicy(OperatorLeaseExpiryPolicyError),
    InvalidFixture(FixtureError),
    InvalidAdministrator(football_application::identity::DevelopmentAdministratorError),
    InvalidPersonName(PersonNameError),
    InvalidTeamName(TeamNameError),
    Configuration(football_platform::ConfigError),
    DatabaseConnection(football_infrastructure::DatabasePoolError),
    AdminUserRepository(football_infrastructure::AdminUserRepositoryError),
    CompetitionRepository(CompetitionRepositoryError),
    FixtureRepository(FixtureRepositoryError),
    PlayerRepository(PlayerRepositoryError),
    TeamRepository(TeamRepositoryError),
    OperationsRepository(OperatorAssignmentRepositoryError),
    ExistingAdministratorConflict { id: AdminUserId },
    ReadBackMissing { id: AdminUserId },
    ReadBackMismatch { id: AdminUserId },
    SameTeamId,
    PlayerReadBackMissing { id: PlayerId, team_id: TeamId },
    PlayerReadBackMismatch { id: PlayerId, team_id: TeamId },
    TeamReadBackMissing { id: TeamId },
    TeamReadBackMismatch { id: TeamId },
    CompetitionReadBackMissing { id: CompetitionId },
    CompetitionReadBackMismatch { id: CompetitionId },
    SeasonReadBackMissing { id: SeasonId },
    SeasonReadBackMismatch { id: SeasonId },
    FixtureSeasonNotFound { id: SeasonId },
    FixtureReadBackMissing { id: FixtureId },
    FixtureReadBackMismatch { id: FixtureId },
    AssignmentReadBackMissing { id: MatchId },
    AssignmentReadBackMismatch { id: MatchId },
    LeaseReadBackMissing { id: OperatorLeaseId },
    LeaseReadBackMismatch { id: OperatorLeaseId },
    HeartbeatReadBackMissing { id: OperatorLeaseId },
    HeartbeatReadBackMismatch { id: OperatorLeaseId },
    ReleaseReadBackMismatch { id: OperatorLeaseId },
    TakeoverReadBackMissing { id: OperatorLeaseId },
    TakeoverReadBackMismatch { id: OperatorLeaseId },
}

impl From<auth::AuthorizationError> for CliError {
    fn from(error: auth::AuthorizationError) -> Self {
        match error.scope() {
            AuthorizationScope::DevelopmentAdmin => Self::SeedNotAuthorized,
            AuthorizationScope::Directory => Self::DirectorySeedNotAuthorized,
            AuthorizationScope::Competition => Self::CompetitionSeedNotAuthorized,
            AuthorizationScope::Scheduling => Self::SchedulingSeedNotAuthorized,
            AuthorizationScope::Operations => Self::OperationsSeedNotAuthorized,
        }
    }
}

impl fmt::Display for CliError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Usage => formatter.write_str("usage: footballctl admin seed-development ... | footballctl directory seed-teams ... | footballctl directory seed-player ... | footballctl directory import-teams ... | footballctl directory import-player ... | footballctl competition seed ... | footballctl season seed ... | footballctl fixture schedule ... | footballctl match assign ... | footballctl match claim ... | footballctl match heartbeat ... | footballctl match release ... | footballctl match takeover ..."),
            Self::InvalidUuid => formatter.write_str("--id must be a valid UUID"),
            Self::UuidMustBeV7 => formatter.write_str("--id values must be UUIDv7"),
            Self::SeedNotAuthorized => auth::AuthorizationError::for_scope(
                AuthorizationScope::DevelopmentAdmin,
            )
            .fmt(formatter),
            Self::DirectorySeedNotAuthorized => {
                auth::AuthorizationError::for_scope(AuthorizationScope::Directory).fmt(formatter)
            }
            Self::CompetitionSeedNotAuthorized => {
                auth::AuthorizationError::for_scope(AuthorizationScope::Competition).fmt(formatter)
            }
            Self::SchedulingSeedNotAuthorized => {
                auth::AuthorizationError::for_scope(AuthorizationScope::Scheduling).fmt(formatter)
            }
            Self::OperationsSeedNotAuthorized => {
                auth::AuthorizationError::for_scope(AuthorizationScope::Operations).fmt(formatter)
            }
            Self::InvalidCompetitionName(error) => error.fmt(formatter),
            Self::InvalidSeasonYear(error) => error.fmt(formatter),
            Self::InvalidSeasonYearValue => formatter.write_str("--year must be a positive integer"),
            Self::InvalidKickoff(error) => write!(
                formatter,
                "--kickoff must be an RFC3339 timestamp with an explicit offset: {error}"
            ),
            Self::InvalidCurrentTime(error) => {
                write!(formatter, "current UTC time could not be normalized: {error}")
            }
            Self::InvalidLeaseSeconds => {
                formatter.write_str("--lease-seconds must be a positive integer")
            }
            Self::InvalidLeaseExpiryPolicy(error) => error.fmt(formatter),
            Self::InvalidFixture(error) => error.fmt(formatter),
            Self::InvalidAdministrator(error) => error.fmt(formatter),
            Self::InvalidPersonName(error) => error.fmt(formatter),
            Self::InvalidTeamName(error) => error.fmt(formatter),
            Self::Configuration(error) => error.fmt(formatter),
            Self::DatabaseConnection(error) => error.fmt(formatter),
            Self::AdminUserRepository(error) => error.fmt(formatter),
            Self::CompetitionRepository(error) => error.fmt(formatter),
            Self::FixtureRepository(error) => error.fmt(formatter),
            Self::PlayerRepository(error) => error.fmt(formatter),
            Self::TeamRepository(error) => error.fmt(formatter),
            Self::OperationsRepository(error) => error.fmt(formatter),
            Self::ExistingAdministratorConflict { id } => write!(
                formatter,
                "administrator {id} already exists with different data or is inactive"
            ),
            Self::ReadBackMissing { id } => {
                write!(formatter, "administrator {id} was not readable after insert")
            }
            Self::ReadBackMismatch { id } => {
                write!(formatter, "administrator {id} read-back did not match insert")
            }
            Self::SameTeamId => formatter.write_str("the two team IDs must be different"),
            Self::PlayerReadBackMissing { id, team_id } => write!(
                formatter,
                "player {id} was not readable after assignment to team {team_id}"
            ),
            Self::PlayerReadBackMismatch { id, team_id } => write!(
                formatter,
                "player {id} read-back did not match assignment to team {team_id}"
            ),
            Self::TeamReadBackMissing { id } => {
                write!(formatter, "team {id} was not readable after insert")
            }
            Self::TeamReadBackMismatch { id } => {
                write!(formatter, "team {id} read-back did not match insert")
            }
            Self::CompetitionReadBackMissing { id } => {
                write!(formatter, "competition {id} was not readable after insert")
            }
            Self::CompetitionReadBackMismatch { id } => {
                write!(formatter, "competition {id} read-back did not match insert")
            }
            Self::SeasonReadBackMissing { id } => {
                write!(formatter, "season {id} was not readable after insert")
            }
            Self::SeasonReadBackMismatch { id } => {
                write!(formatter, "season {id} read-back did not match insert")
            }
            Self::FixtureSeasonNotFound { id } => write!(formatter, "season {id} does not exist"),
            Self::FixtureReadBackMissing { id } => {
                write!(formatter, "fixture {id} was not readable after scheduling")
            }
            Self::FixtureReadBackMismatch { id } => {
                write!(formatter, "fixture {id} read-back did not match scheduling")
            }
            Self::AssignmentReadBackMissing { id } => write!(
                formatter,
                "match assignment {id} was not readable after persistence"
            ),
            Self::AssignmentReadBackMismatch { id } => write!(
                formatter,
                "match assignment {id} read-back did not match persistence"
            ),
            Self::LeaseReadBackMissing { id } => {
                write!(formatter, "operator lease {id} was not readable after claim")
            }
            Self::LeaseReadBackMismatch { id } => {
                write!(formatter, "operator lease {id} read-back did not match claim")
            }
            Self::HeartbeatReadBackMissing { id } => write!(
                formatter,
                "operator lease {id} was not readable after heartbeat"
            ),
            Self::HeartbeatReadBackMismatch { id } => write!(
                formatter,
                "operator lease {id} read-back did not match heartbeat"
            ),
            Self::ReleaseReadBackMismatch { id } => {
                write!(formatter, "operator lease {id} still exists after release")
            }
            Self::TakeoverReadBackMissing { id } => write!(
                formatter,
                "operator lease {id} was not readable after takeover"
            ),
            Self::TakeoverReadBackMismatch { id } => write!(
                formatter,
                "operator lease {id} read-back did not match takeover"
            ),
        }
    }
}

impl Error for CliError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::InvalidCompetitionName(error) => Some(error),
            Self::InvalidSeasonYear(error) => Some(error),
            Self::InvalidKickoff(error) => Some(error),
            Self::InvalidCurrentTime(error) => Some(error),
            Self::InvalidLeaseExpiryPolicy(error) => Some(error),
            Self::InvalidFixture(error) => Some(error),
            Self::InvalidAdministrator(error) => Some(error),
            Self::InvalidPersonName(error) => Some(error),
            Self::InvalidTeamName(error) => Some(error),
            Self::Configuration(error) => Some(error),
            Self::DatabaseConnection(error) => Some(error),
            Self::AdminUserRepository(error) => Some(error),
            Self::CompetitionRepository(error) => Some(error),
            Self::FixtureRepository(error) => Some(error),
            Self::PlayerRepository(error) => Some(error),
            Self::TeamRepository(error) => Some(error),
            Self::OperationsRepository(error) => Some(error),
            _ => None,
        }
    }
}
