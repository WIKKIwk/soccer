use axum::{
    Extension, Json, Router,
    extract::{Path, State, rejection::JsonRejection},
    http::{HeaderMap, StatusCode},
    routing::post,
};
use football_application::match_command_fingerprint::fingerprint_lifecycle_command;
use football_contracts::{
    AdminCommandResponse, LifecycleCommandRequest, LineupSubmitCommandRequest,
};
use football_domain::{
    ActorIdentity, LifecycleEventPayload, LineupId, MatchId, MatchLifecycleCommand,
};
use football_infrastructure::{
    DatabasePool, dispatch_lifecycle_command, load_lineup_submission, require_active_match_operator,
};
use football_platform::RequestId;

use crate::{
    auth::development_actor,
    command_input::{ParsedCommandMetadata, current_utc_instant, parse_match_id, parse_uuidv7},
    command_response::lifecycle_response,
    error::ApiCommandError,
};

pub(crate) fn router() -> Router<DatabasePool> {
    Router::new()
        .route("/admin/v1/matches/{match_id}/schedule", post(schedule))
        .route(
            "/admin/v1/matches/{match_id}/lineups/submit",
            post(submit_lineup),
        )
        .route(
            "/admin/v1/matches/{match_id}/lineups/confirm",
            post(confirm_lineups),
        )
        .route(
            "/admin/v1/matches/{match_id}/periods/first-half/start",
            post(start_first_half),
        )
        .route(
            "/admin/v1/matches/{match_id}/periods/first-half/end",
            post(end_first_half),
        )
        .route(
            "/admin/v1/matches/{match_id}/periods/second-half/start",
            post(start_second_half),
        )
        .route("/admin/v1/matches/{match_id}/finish", post(finish))
}

async fn schedule(
    State(pool): State<DatabasePool>,
    Extension(request_id): Extension<RequestId>,
    Path(match_id): Path<String>,
    headers: HeaderMap,
    payload: Result<Json<LifecycleCommandRequest>, JsonRejection>,
) -> Result<(StatusCode, Json<AdminCommandResponse>), ApiCommandError> {
    execute_simple(
        &pool,
        request_id,
        &match_id,
        &headers,
        payload
            .map_err(|error| ApiCommandError::invalid_json(request_id, error))?
            .0,
        LifecycleEventPayload::MatchScheduled,
    )
    .await
}

async fn confirm_lineups(
    State(pool): State<DatabasePool>,
    Extension(request_id): Extension<RequestId>,
    Path(match_id): Path<String>,
    headers: HeaderMap,
    payload: Result<Json<LifecycleCommandRequest>, JsonRejection>,
) -> Result<(StatusCode, Json<AdminCommandResponse>), ApiCommandError> {
    execute_simple(
        &pool,
        request_id,
        &match_id,
        &headers,
        payload
            .map_err(|error| ApiCommandError::invalid_json(request_id, error))?
            .0,
        LifecycleEventPayload::LineupConfirmed,
    )
    .await
}

async fn start_first_half(
    State(pool): State<DatabasePool>,
    Extension(request_id): Extension<RequestId>,
    Path(match_id): Path<String>,
    headers: HeaderMap,
    payload: Result<Json<LifecycleCommandRequest>, JsonRejection>,
) -> Result<(StatusCode, Json<AdminCommandResponse>), ApiCommandError> {
    execute_simple(
        &pool,
        request_id,
        &match_id,
        &headers,
        payload
            .map_err(|error| ApiCommandError::invalid_json(request_id, error))?
            .0,
        LifecycleEventPayload::FirstHalfStarted,
    )
    .await
}

async fn end_first_half(
    State(pool): State<DatabasePool>,
    Extension(request_id): Extension<RequestId>,
    Path(match_id): Path<String>,
    headers: HeaderMap,
    payload: Result<Json<LifecycleCommandRequest>, JsonRejection>,
) -> Result<(StatusCode, Json<AdminCommandResponse>), ApiCommandError> {
    execute_simple(
        &pool,
        request_id,
        &match_id,
        &headers,
        payload
            .map_err(|error| ApiCommandError::invalid_json(request_id, error))?
            .0,
        LifecycleEventPayload::FirstHalfEnded,
    )
    .await
}

async fn start_second_half(
    State(pool): State<DatabasePool>,
    Extension(request_id): Extension<RequestId>,
    Path(match_id): Path<String>,
    headers: HeaderMap,
    payload: Result<Json<LifecycleCommandRequest>, JsonRejection>,
) -> Result<(StatusCode, Json<AdminCommandResponse>), ApiCommandError> {
    execute_simple(
        &pool,
        request_id,
        &match_id,
        &headers,
        payload
            .map_err(|error| ApiCommandError::invalid_json(request_id, error))?
            .0,
        LifecycleEventPayload::SecondHalfStarted,
    )
    .await
}

async fn finish(
    State(pool): State<DatabasePool>,
    Extension(request_id): Extension<RequestId>,
    Path(match_id): Path<String>,
    headers: HeaderMap,
    payload: Result<Json<LifecycleCommandRequest>, JsonRejection>,
) -> Result<(StatusCode, Json<AdminCommandResponse>), ApiCommandError> {
    execute_simple(
        &pool,
        request_id,
        &match_id,
        &headers,
        payload
            .map_err(|error| ApiCommandError::invalid_json(request_id, error))?
            .0,
        LifecycleEventPayload::MatchFinished,
    )
    .await
}

async fn execute_simple(
    pool: &DatabasePool,
    request_id: RequestId,
    match_id: &str,
    headers: &HeaderMap,
    request: LifecycleCommandRequest,
    action: LifecycleEventPayload,
) -> Result<(StatusCode, Json<AdminCommandResponse>), ApiCommandError> {
    let actor = development_actor(headers, request_id)?;
    let match_id = parse_match_id(
        match_id,
        request_id,
        Some(&request.command.client_command_id),
    )?;
    execute_action(pool, request_id, match_id, actor, &request.command, action).await
}

async fn submit_lineup(
    State(pool): State<DatabasePool>,
    Extension(request_id): Extension<RequestId>,
    Path(match_id): Path<String>,
    headers: HeaderMap,
    payload: Result<Json<LineupSubmitCommandRequest>, JsonRejection>,
) -> Result<(StatusCode, Json<AdminCommandResponse>), ApiCommandError> {
    let request = payload
        .map_err(|error| ApiCommandError::invalid_json(request_id, error))?
        .0;
    let actor = development_actor(&headers, request_id)?;
    let client_id = Some(request.command.client_command_id.as_str());
    let match_id = parse_match_id(&match_id, request_id, client_id)?;
    let lineup_id = LineupId::from_uuid(parse_uuidv7(
        &request.lineup_id,
        "lineup_id",
        request_id,
        client_id,
    )?);
    let action = LifecycleEventPayload::LineupSubmitted(
        load_lineup_submission(&pool, match_id, lineup_id)
            .await
            .map_err(|error| {
                ApiCommandError::new(error.to_application_error(), request_id, client_id)
            })?,
    );
    execute_action(&pool, request_id, match_id, actor, &request.command, action).await
}

async fn execute_action(
    pool: &DatabasePool,
    request_id: RequestId,
    match_id: MatchId,
    actor: ActorIdentity,
    metadata: &football_contracts::CommandMetadata,
    action: LifecycleEventPayload,
) -> Result<(StatusCode, Json<AdminCommandResponse>), ApiCommandError> {
    let parsed = ParsedCommandMetadata::parse(metadata, request_id)?;
    let now_utc = current_utc_instant(request_id, Some(&metadata.client_command_id))?;
    require_active_match_operator(pool, match_id, actor, parsed.operator_lease_id, now_utc)
        .await
        .map_err(|error| {
            ApiCommandError::new(
                error.to_application_error(),
                request_id,
                Some(&metadata.client_command_id),
            )
        })?;
    let command = MatchLifecycleCommand::new(
        parsed.command_id,
        parsed.event_id,
        match_id,
        parsed.expected_version,
        parsed.clock_ms,
        parsed.occurred_at,
        now_utc,
        actor,
        action,
    );
    let fingerprint =
        fingerprint_lifecycle_command(parsed.client_command_id, parsed.operator_lease_id, &command);
    let result = dispatch_lifecycle_command(
        pool,
        parsed.dispatch(fingerprint, now_utc, request_id)?,
        &command,
    )
    .await
    .map_err(|error| {
        ApiCommandError::new(
            error.to_application_error(),
            request_id,
            Some(&metadata.client_command_id),
        )
    })?;
    Ok((
        StatusCode::OK,
        Json(lifecycle_response(
            result,
            request_id,
            parsed.client_command_id,
        )),
    ))
}
