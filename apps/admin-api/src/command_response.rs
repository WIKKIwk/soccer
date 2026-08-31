use football_application::commands::{CommandOrchestrationResult, PersistedCommandResult};
use football_contracts::AdminCommandResponse;
use football_domain::{C1LiveActionCommandError, CommandOutcome, MatchLifecycleCommandError};
use football_infrastructure::{
    C1LiveActionExecutionError, C1LiveActionResult, LifecycleCommandResult, LifecycleExecutionError,
};
use football_platform::RequestId;
use serde_json::{Value, json};

pub(crate) fn lifecycle_response(
    result: CommandOrchestrationResult<LifecycleCommandResult, MatchLifecycleCommandError>,
    request_id: RequestId,
    client_command_id: football_domain::ClientCommandId,
) -> AdminCommandResponse {
    match result {
        CommandOrchestrationResult::ExistingTerminal { result } => {
            persisted_response(result, request_id, client_command_id)
        }
        CommandOrchestrationResult::Executed(envelope) => {
            let command_id = envelope.command_id();
            let match_id = envelope.match_id();
            match envelope.into_outcome() {
                CommandOutcome::Accepted {
                    final_aggregate_version,
                    event_count,
                    result,
                } => AdminCommandResponse::accepted(
                    request_id.to_string(),
                    client_command_id.to_string(),
                    command_id.to_string(),
                    match_id.to_string(),
                    final_aggregate_version.value(),
                    event_count,
                    json!({
                        "event_id": result.event_id.to_string(),
                        "event_type": result.event_type.as_str(),
                        "match_state": result.match_state.as_persisted_str(),
                    }),
                ),
                CommandOutcome::Rejected {
                    aggregate_version,
                    error,
                } => {
                    let application =
                        LifecycleExecutionError::Command(error).to_application_error();
                    AdminCommandResponse::rejected(
                        request_id.to_string(),
                        client_command_id.to_string(),
                        command_id.to_string(),
                        match_id.to_string(),
                        aggregate_version.value(),
                        application.code().as_str(),
                        details_value(application.details()),
                    )
                }
            }
        }
    }
}

pub(crate) fn c1_response(
    result: CommandOrchestrationResult<C1LiveActionResult, C1LiveActionCommandError>,
    request_id: RequestId,
    client_command_id: football_domain::ClientCommandId,
) -> AdminCommandResponse {
    match result {
        CommandOrchestrationResult::ExistingTerminal { result } => {
            persisted_response(result, request_id, client_command_id)
        }
        CommandOrchestrationResult::Executed(envelope) => {
            let command_id = envelope.command_id();
            let match_id = envelope.match_id();
            match envelope.into_outcome() {
                CommandOutcome::Accepted {
                    final_aggregate_version,
                    event_count,
                    result,
                } => AdminCommandResponse::accepted(
                    request_id.to_string(),
                    client_command_id.to_string(),
                    command_id.to_string(),
                    match_id.to_string(),
                    final_aggregate_version.value(),
                    event_count,
                    json!({
                        "event_id": result.event_id().to_string(),
                        "event_type": result.event_type().as_str(),
                    }),
                ),
                CommandOutcome::Rejected {
                    aggregate_version,
                    error,
                } => {
                    let application =
                        C1LiveActionExecutionError::Command(error).to_application_error();
                    AdminCommandResponse::rejected(
                        request_id.to_string(),
                        client_command_id.to_string(),
                        command_id.to_string(),
                        match_id.to_string(),
                        aggregate_version.value(),
                        application.code().as_str(),
                        details_value(application.details()),
                    )
                }
            }
        }
    }
}

fn persisted_response(
    result: PersistedCommandResult,
    request_id: RequestId,
    client_command_id: football_domain::ClientCommandId,
) -> AdminCommandResponse {
    match result {
        PersistedCommandResult::Accepted {
            command_id,
            aggregate_id,
            final_aggregate_version,
            event_count,
            result_payload,
            ..
        } => AdminCommandResponse::accepted(
            request_id.to_string(),
            client_command_id.to_string(),
            command_id.to_string(),
            aggregate_id.to_string(),
            final_aggregate_version.value(),
            event_count,
            result_payload,
        ),
        PersistedCommandResult::Rejected {
            command_id,
            aggregate_id,
            aggregate_version,
            rejection_code,
            rejection_details,
            ..
        } => AdminCommandResponse::rejected(
            request_id.to_string(),
            client_command_id.to_string(),
            command_id.to_string(),
            aggregate_id.to_string(),
            aggregate_version.value(),
            rejection_code,
            rejection_details,
        ),
    }
}

fn details_value(details: &std::collections::BTreeMap<String, String>) -> Value {
    serde_json::to_value(details).expect("string-keyed string details always serialize")
}
