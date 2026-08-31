use crate::{
    auth::{self, AuthorizationScope},
    config::CliRuntimeConfig,
    error::CliError,
    live_arguments::{
        CardKind, LiveCommandAction, LiveCommandMetadata, parse_draft_lineup, parse_live_command,
    },
    live_error::LiveCommandError,
};
use football_application::{
    commands::{CommandOrchestrationResult, PersistedCommandResult},
    match_command_fingerprint::{fingerprint_c1_command, fingerprint_lifecycle_command},
};
use football_domain::{
    ActorIdentity, C1LiveAction, C1LiveActionCommand, CommandOutcome, EventVerificationStatus,
    LifecycleEventPayload, MatchLifecycleCommand, RedCardRecordedPayload,
    SecondYellowCardRecordedPayload, SubstitutionRecordedPayload, UtcInstant,
    YellowCardRecordedPayload,
};
use football_infrastructure::{
    DatabasePool, MatchCommandDispatch, dispatch_c1_command, dispatch_lifecycle_command,
    load_draft_lineup, load_fixture, load_lineup_submission, persist_draft_lineup,
    require_active_match_operator,
};
use time::{Duration, OffsetDateTime};
use uuid::Uuid;

const COMMAND_PROCESSING_LEASE_SECONDS: i64 = 30;

pub(crate) async fn run_lineup<I>(arguments: I) -> Result<(), CliError>
where
    I: IntoIterator<Item = String>,
{
    let arguments = parse_draft_lineup(arguments).map_err(CliError::from)?;
    auth::require(AuthorizationScope::Scheduling).map_err(CliError::from)?;
    let pool = connect_database().await?;
    let fixture = load_fixture(&pool, arguments.lineup.fixture_id())
        .await
        .map_err(CliError::FixtureRepository)?
        .ok_or_else(|| LiveCommandError::invalid("--fixture-id", "fixture does not exist"))?;
    let persisted = persist_draft_lineup(&pool, &fixture, &arguments.lineup)
        .await
        .map_err(LiveCommandError::LineupRepository)?;
    let loaded = load_draft_lineup(&pool, persisted.id())
        .await
        .map_err(LiveCommandError::LineupRepository)?
        .ok_or_else(|| {
            LiveCommandError::invalid("--lineup-id", "lineup was not readable after save")
        })?;
    if loaded != persisted {
        return Err(LiveCommandError::invalid(
            "--lineup-id",
            "lineup read-back did not match the saved draft",
        )
        .into());
    }
    println!(
        "draft lineup persisted and verified: {} team={} entries={}",
        persisted.id(),
        persisted.team_id(),
        persisted.entries().len()
    );
    pool.close().await;
    Ok(())
}

pub(crate) async fn run_match<I>(arguments: I) -> Result<(), CliError>
where
    I: IntoIterator<Item = String>,
{
    let arguments = parse_live_command(arguments).map_err(CliError::from)?;
    auth::require(AuthorizationScope::Operations).map_err(CliError::from)?;
    let pool = connect_database().await?;
    let now_utc = current_utc_instant()?;
    let actor = ActorIdentity::administrator(arguments.metadata.operator_id);
    require_active_match_operator(
        &pool,
        arguments.metadata.match_id,
        actor,
        arguments.metadata.operator_lease_id,
        now_utc,
    )
    .await
    .map_err(|error| LiveCommandError::Application(error.to_application_error()))?;

    match arguments.action {
        LiveCommandAction::Goal {
            period,
            team_id,
            scorer_player_id,
            award_type,
        } => {
            execute_c1(
                &pool,
                arguments.metadata,
                now_utc,
                period,
                C1LiveAction::RecordGoal(football_domain::GoalRecordedPayload::new(
                    team_id,
                    scorer_player_id,
                    award_type,
                )),
            )
            .await?;
        }
        LiveCommandAction::Card {
            period,
            player_id,
            card,
        } => {
            let action = match card {
                CardKind::Yellow => {
                    C1LiveAction::RecordYellowCard(YellowCardRecordedPayload::new(player_id))
                }
                CardKind::SecondYellow => C1LiveAction::RecordSecondYellowCard(
                    SecondYellowCardRecordedPayload::new(player_id),
                ),
                CardKind::Red => {
                    C1LiveAction::RecordRedCard(RedCardRecordedPayload::new(player_id))
                }
            };
            execute_c1(&pool, arguments.metadata, now_utc, period, action).await?;
        }
        LiveCommandAction::Substitution {
            period,
            team_id,
            player_on_id,
            player_off_id,
        } => {
            let payload =
                SubstitutionRecordedPayload::try_new(team_id, player_on_id, player_off_id)
                    .map_err(|error| {
                        LiveCommandError::invalid("--player-on-id", error.to_string())
                    })?;
            execute_c1(
                &pool,
                arguments.metadata,
                now_utc,
                period,
                C1LiveAction::RecordSubstitution(payload),
            )
            .await?;
        }
        action => {
            let payload = lifecycle_payload(&pool, arguments.metadata, action).await?;
            execute_lifecycle(&pool, arguments.metadata, now_utc, payload).await?;
        }
    }
    pool.close().await;
    Ok(())
}

async fn lifecycle_payload(
    pool: &DatabasePool,
    metadata: LiveCommandMetadata,
    action: LiveCommandAction,
) -> Result<LifecycleEventPayload, CliError> {
    match action {
        LiveCommandAction::Schedule => Ok(LifecycleEventPayload::MatchScheduled),
        LiveCommandAction::SubmitLineup(lineup_id) => {
            load_lineup_submission(pool, metadata.match_id, lineup_id)
                .await
                .map(LifecycleEventPayload::LineupSubmitted)
                .map_err(|error| LiveCommandError::Application(error.to_application_error()).into())
        }
        LiveCommandAction::ConfirmLineups => Ok(LifecycleEventPayload::LineupConfirmed),
        LiveCommandAction::StartFirstHalf => Ok(LifecycleEventPayload::FirstHalfStarted),
        LiveCommandAction::EndFirstHalf => Ok(LifecycleEventPayload::FirstHalfEnded),
        LiveCommandAction::StartSecondHalf => Ok(LifecycleEventPayload::SecondHalfStarted),
        LiveCommandAction::Finish => Ok(LifecycleEventPayload::MatchFinished),
        LiveCommandAction::Goal { .. }
        | LiveCommandAction::Card { .. }
        | LiveCommandAction::Substitution { .. } => Err(LiveCommandError::invalid(
            "command",
            "live action was routed to the lifecycle executor",
        )
        .into()),
    }
}

async fn execute_lifecycle(
    pool: &DatabasePool,
    metadata: LiveCommandMetadata,
    now_utc: UtcInstant,
    action: LifecycleEventPayload,
) -> Result<(), CliError> {
    let command = MatchLifecycleCommand::new(
        metadata.command_id,
        metadata.event_id,
        metadata.match_id,
        metadata.expected_version,
        metadata.clock_ms,
        metadata.occurred_at,
        now_utc,
        ActorIdentity::administrator(metadata.operator_id),
        action,
    );
    let fingerprint = fingerprint_lifecycle_command(
        metadata.client_command_id,
        metadata.operator_lease_id,
        &command,
    );
    let result =
        dispatch_lifecycle_command(pool, dispatch(metadata, fingerprint, now_utc)?, &command)
            .await
            .map_err(|error| LiveCommandError::Application(error.to_application_error()))?;
    print_lifecycle_result(result);
    Ok(())
}

async fn execute_c1(
    pool: &DatabasePool,
    metadata: LiveCommandMetadata,
    now_utc: UtcInstant,
    period: football_domain::MatchPeriod,
    action: C1LiveAction,
) -> Result<(), CliError> {
    let command = C1LiveActionCommand::try_new(
        metadata.command_id,
        metadata.event_id,
        metadata.match_id,
        metadata.expected_version,
        period,
        metadata.clock_ms,
        metadata.occurred_at,
        now_utc,
        ActorIdentity::administrator(metadata.operator_id),
        EventVerificationStatus::Provisional,
        action,
    )
    .map_err(|error| LiveCommandError::invalid("--period", error.to_string()))?;
    let fingerprint = fingerprint_c1_command(
        metadata.client_command_id,
        metadata.operator_lease_id,
        &command,
    );
    let result = dispatch_c1_command(pool, dispatch(metadata, fingerprint, now_utc)?, &command)
        .await
        .map_err(|error| LiveCommandError::Application(error.to_application_error()))?;
    print_c1_result(result);
    Ok(())
}

fn dispatch(
    metadata: LiveCommandMetadata,
    fingerprint: football_domain::RequestFingerprint,
    now_utc: UtcInstant,
) -> Result<MatchCommandDispatch, LiveCommandError> {
    let expires_at = now_utc
        .as_offset_datetime()
        .checked_add(Duration::seconds(COMMAND_PROCESSING_LEASE_SECONDS))
        .map(UtcInstant::from_offset_datetime)
        .ok_or(LiveCommandError::ProcessingLeaseOutOfRange)?;
    Ok(MatchCommandDispatch::new(
        metadata.client_command_id,
        fingerprint,
        Uuid::now_v7(),
        now_utc,
        expires_at,
    ))
}

fn print_lifecycle_result(
    result: CommandOrchestrationResult<
        football_infrastructure::LifecycleCommandResult,
        football_domain::MatchLifecycleCommandError,
    >,
) {
    match result {
        CommandOrchestrationResult::ExistingTerminal { result } => print_persisted(result),
        CommandOrchestrationResult::Executed(envelope) => match envelope.into_outcome() {
            CommandOutcome::Accepted {
                final_aggregate_version,
                result,
                ..
            } => println!(
                "command accepted: event={} type={} state={} version={}",
                result.event_id,
                result.event_type.as_str(),
                result.match_state.as_persisted_str(),
                final_aggregate_version.value()
            ),
            CommandOutcome::Rejected {
                aggregate_version,
                error,
            } => println!(
                "command rejected: version={} reason={error}",
                aggregate_version.value()
            ),
        },
    }
}

fn print_c1_result(
    result: CommandOrchestrationResult<
        football_infrastructure::C1LiveActionResult,
        football_domain::C1LiveActionCommandError,
    >,
) {
    match result {
        CommandOrchestrationResult::ExistingTerminal { result } => print_persisted(result),
        CommandOrchestrationResult::Executed(envelope) => match envelope.into_outcome() {
            CommandOutcome::Accepted {
                final_aggregate_version,
                result,
                ..
            } => println!(
                "command accepted: event={} type={} version={}",
                result.event_id(),
                result.event_type().as_str(),
                final_aggregate_version.value()
            ),
            CommandOutcome::Rejected {
                aggregate_version,
                error,
            } => println!(
                "command rejected: version={} reason={error}",
                aggregate_version.value()
            ),
        },
    }
}

fn print_persisted(result: PersistedCommandResult) {
    match result {
        PersistedCommandResult::Accepted {
            command_id,
            final_aggregate_version,
            ..
        } => println!(
            "command already accepted: {command_id} version={}",
            final_aggregate_version.value()
        ),
        PersistedCommandResult::Rejected {
            command_id,
            aggregate_version,
            rejection_code,
            ..
        } => println!(
            "command already rejected: {command_id} version={} code={rejection_code}",
            aggregate_version.value()
        ),
    }
}

async fn connect_database() -> Result<DatabasePool, CliError> {
    let config = CliRuntimeConfig::from_env().map_err(CliError::Configuration)?;
    config
        .connect_database()
        .await
        .map_err(CliError::DatabaseConnection)
}

fn current_utc_instant() -> Result<UtcInstant, LiveCommandError> {
    let now = OffsetDateTime::now_utc();
    let microseconds = (now.nanosecond() / 1_000) * 1_000;
    now.replace_nanosecond(microseconds)
        .map(UtcInstant::from_offset_datetime)
        .map_err(LiveCommandError::InvalidCurrentTime)
}
