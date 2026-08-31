use axum::{
    Extension, Json, Router,
    extract::{Path, State, rejection::JsonRejection},
    http::{HeaderMap, StatusCode},
    routing::post,
};
use football_application::match_command_fingerprint::fingerprint_c1_command;
use football_contracts::{
    AdminCommandResponse, CardCommandRequest, CardKind, CommandMetadata, GoalAwardKind,
    GoalCommandRequest, LivePeriodKind, SubstitutionCommandRequest,
};
use football_domain::{
    ActorIdentity, C1LiveAction, C1LiveActionCommand, EventVerificationStatus, GoalAwardType,
    GoalRecordedPayload, MatchId, MatchPeriod, PlayerId, RedCardRecordedPayload,
    SecondYellowCardRecordedPayload, SubstitutionRecordedPayload, TeamId,
    YellowCardRecordedPayload,
};
use football_infrastructure::{DatabasePool, dispatch_c1_command, require_active_match_operator};
use football_platform::RequestId;

use crate::{
    auth::development_actor,
    command_input::{ParsedCommandMetadata, current_utc_instant, parse_match_id, parse_uuidv7},
    command_response::c1_response,
    error::ApiCommandError,
};

pub(crate) fn router() -> Router<DatabasePool> {
    Router::new()
        .route("/admin/v1/matches/{match_id}/goals", post(record_goal))
        .route("/admin/v1/matches/{match_id}/cards", post(record_card))
        .route(
            "/admin/v1/matches/{match_id}/substitutions",
            post(record_substitution),
        )
}

struct PreparedCommand {
    actor: ActorIdentity,
    match_id: MatchId,
    metadata: ParsedCommandMetadata,
    now_utc: football_domain::UtcInstant,
}

async fn record_goal(
    State(pool): State<DatabasePool>,
    Extension(request_id): Extension<RequestId>,
    Path(match_id): Path<String>,
    headers: HeaderMap,
    payload: Result<Json<GoalCommandRequest>, JsonRejection>,
) -> Result<(StatusCode, Json<AdminCommandResponse>), ApiCommandError> {
    let request = payload
        .map_err(|error| ApiCommandError::invalid_json(request_id, error))?
        .0;
    let prepared = prepare(&pool, request_id, &match_id, &headers, &request.command).await?;
    let client_id = Some(request.command.client_command_id.as_str());
    let team_id = TeamId::from_uuid(parse_uuidv7(
        &request.team_id,
        "team_id",
        request_id,
        client_id,
    )?);
    let scorer_player_id = request
        .scorer_player_id
        .as_deref()
        .map(|value| parse_uuidv7(value, "scorer_player_id", request_id, client_id))
        .transpose()?
        .map(PlayerId::from_uuid);
    let award_type = match request.award_type {
        GoalAwardKind::Regular => GoalAwardType::Regular,
        GoalAwardKind::Penalty => GoalAwardType::Penalty,
        GoalAwardKind::OwnGoal => GoalAwardType::OwnGoal,
    };
    execute(
        &pool,
        request_id,
        &request.command,
        prepared,
        period(request.period),
        C1LiveAction::RecordGoal(GoalRecordedPayload::new(
            team_id,
            scorer_player_id,
            award_type,
        )),
    )
    .await
}

async fn record_card(
    State(pool): State<DatabasePool>,
    Extension(request_id): Extension<RequestId>,
    Path(match_id): Path<String>,
    headers: HeaderMap,
    payload: Result<Json<CardCommandRequest>, JsonRejection>,
) -> Result<(StatusCode, Json<AdminCommandResponse>), ApiCommandError> {
    let request = payload
        .map_err(|error| ApiCommandError::invalid_json(request_id, error))?
        .0;
    let prepared = prepare(&pool, request_id, &match_id, &headers, &request.command).await?;
    let player_id = PlayerId::from_uuid(parse_uuidv7(
        &request.player_id,
        "player_id",
        request_id,
        Some(&request.command.client_command_id),
    )?);
    let action = match request.card {
        CardKind::Yellow => {
            C1LiveAction::RecordYellowCard(YellowCardRecordedPayload::new(player_id))
        }
        CardKind::SecondYellow => {
            C1LiveAction::RecordSecondYellowCard(SecondYellowCardRecordedPayload::new(player_id))
        }
        CardKind::Red => C1LiveAction::RecordRedCard(RedCardRecordedPayload::new(player_id)),
    };
    execute(
        &pool,
        request_id,
        &request.command,
        prepared,
        period(request.period),
        action,
    )
    .await
}

async fn record_substitution(
    State(pool): State<DatabasePool>,
    Extension(request_id): Extension<RequestId>,
    Path(match_id): Path<String>,
    headers: HeaderMap,
    payload: Result<Json<SubstitutionCommandRequest>, JsonRejection>,
) -> Result<(StatusCode, Json<AdminCommandResponse>), ApiCommandError> {
    let request = payload
        .map_err(|error| ApiCommandError::invalid_json(request_id, error))?
        .0;
    let prepared = prepare(&pool, request_id, &match_id, &headers, &request.command).await?;
    let client_id = Some(request.command.client_command_id.as_str());
    let team_id = TeamId::from_uuid(parse_uuidv7(
        &request.team_id,
        "team_id",
        request_id,
        client_id,
    )?);
    let player_on_id = PlayerId::from_uuid(parse_uuidv7(
        &request.player_on_id,
        "player_on_id",
        request_id,
        client_id,
    )?);
    let player_off_id = PlayerId::from_uuid(parse_uuidv7(
        &request.player_off_id,
        "player_off_id",
        request_id,
        client_id,
    )?);
    let substitution = SubstitutionRecordedPayload::try_new(team_id, player_on_id, player_off_id)
        .map_err(|error| {
        ApiCommandError::invalid_value(request_id, client_id, "player_on_id", error.to_string())
    })?;
    execute(
        &pool,
        request_id,
        &request.command,
        prepared,
        period(request.period),
        C1LiveAction::RecordSubstitution(substitution),
    )
    .await
}

async fn prepare(
    pool: &DatabasePool,
    request_id: RequestId,
    match_id: &str,
    headers: &HeaderMap,
    metadata: &CommandMetadata,
) -> Result<PreparedCommand, ApiCommandError> {
    let actor = development_actor(headers, request_id)?;
    let client_id = Some(metadata.client_command_id.as_str());
    let match_id = parse_match_id(match_id, request_id, client_id)?;
    let parsed = ParsedCommandMetadata::parse(metadata, request_id)?;
    let now_utc = current_utc_instant(request_id, client_id)?;
    require_active_match_operator(pool, match_id, actor, parsed.operator_lease_id, now_utc)
        .await
        .map_err(|error| {
            ApiCommandError::new(error.to_application_error(), request_id, client_id)
        })?;
    Ok(PreparedCommand {
        actor,
        match_id,
        metadata: parsed,
        now_utc,
    })
}

async fn execute(
    pool: &DatabasePool,
    request_id: RequestId,
    contract: &CommandMetadata,
    prepared: PreparedCommand,
    period: MatchPeriod,
    action: C1LiveAction,
) -> Result<(StatusCode, Json<AdminCommandResponse>), ApiCommandError> {
    let command = C1LiveActionCommand::try_new(
        prepared.metadata.command_id,
        prepared.metadata.event_id,
        prepared.match_id,
        prepared.metadata.expected_version,
        period,
        prepared.metadata.clock_ms,
        prepared.metadata.occurred_at,
        prepared.now_utc,
        prepared.actor,
        EventVerificationStatus::Provisional,
        action,
    )
    .map_err(|error| {
        ApiCommandError::invalid_value(
            request_id,
            Some(&contract.client_command_id),
            "period",
            error.to_string(),
        )
    })?;
    let fingerprint = fingerprint_c1_command(
        prepared.metadata.client_command_id,
        prepared.metadata.operator_lease_id,
        &command,
    );
    let dispatch = prepared
        .metadata
        .dispatch(fingerprint, prepared.now_utc, request_id)?;
    let result = dispatch_c1_command(pool, dispatch, &command)
        .await
        .map_err(|error| {
            ApiCommandError::new(
                error.to_application_error(),
                request_id,
                Some(&contract.client_command_id),
            )
        })?;
    Ok((
        StatusCode::OK,
        Json(c1_response(
            result,
            request_id,
            prepared.metadata.client_command_id,
        )),
    ))
}

const fn period(value: LivePeriodKind) -> MatchPeriod {
    match value {
        LivePeriodKind::FirstHalf => MatchPeriod::FirstHalf,
        LivePeriodKind::SecondHalf => MatchPeriod::SecondHalf,
    }
}
