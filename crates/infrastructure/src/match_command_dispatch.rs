use football_application::commands::{
    CommandAdmissionRequest, CommandAdmissionRequestError, CommandExecutionContext,
    CommandOrchestrationError, CommandOrchestrationResult, CommandOrchestrator,
    LogicalCommandIdentity,
};
use football_domain::{
    ActorIdentity, ClientCommandId, MatchLifecycleCommand, RequestFingerprint, UtcInstant,
};
use football_platform::{ApplicationError, ErrorCode};
use sqlx::PgPool;
use std::fmt;
use uuid::Uuid;

use crate::{
    C1LiveActionCommandExecutionError, CommandAdmissionError, PostgresC1LiveActionExecutor,
    PostgresCommandAdmission, PostgresLifecycleExecutor,
};

#[derive(Debug, Clone, Copy)]
pub struct MatchCommandDispatch {
    client_command_id: ClientCommandId,
    request_fingerprint: RequestFingerprint,
    processing_owner: Uuid,
    now_utc: UtcInstant,
    processing_expires_at: UtcInstant,
}

impl MatchCommandDispatch {
    pub const fn new(
        client_command_id: ClientCommandId,
        request_fingerprint: RequestFingerprint,
        processing_owner: Uuid,
        now_utc: UtcInstant,
        processing_expires_at: UtcInstant,
    ) -> Self {
        Self {
            client_command_id,
            request_fingerprint,
            processing_owner,
            now_utc,
            processing_expires_at,
        }
    }

    fn request_and_context(
        self,
        command_id: football_domain::CommandId,
        actor: ActorIdentity,
        expected_version: football_domain::AggregateVersion,
    ) -> Result<(CommandAdmissionRequest, CommandExecutionContext), CommandAdmissionRequestError>
    {
        let request = CommandAdmissionRequest::try_new(
            LogicalCommandIdentity::new(
                command_id,
                self.client_command_id,
                self.request_fingerprint,
            ),
            self.processing_owner,
            self.now_utc,
            self.processing_expires_at,
        )?;
        let context =
            CommandExecutionContext::from_admission_request(request, actor, Some(expected_version));
        Ok((request, context))
    }
}

#[derive(Debug)]
pub enum LifecycleDispatchError {
    InvalidAdmission(CommandAdmissionRequestError),
    Orchestration(
        CommandOrchestrationError<CommandAdmissionError, crate::LifecycleCommandExecutionError>,
    ),
}

impl LifecycleDispatchError {
    pub fn to_application_error(&self) -> ApplicationError {
        match self {
            Self::InvalidAdmission(_) => invalid_processing_lease(),
            Self::Orchestration(CommandOrchestrationError::Admission(error)) => {
                error.to_application_error()
            }
            Self::Orchestration(CommandOrchestrationError::Execution(error)) => {
                error.to_application_error()
            }
            Self::Orchestration(_) => integrity_error(),
        }
    }
}

impl fmt::Display for LifecycleDispatchError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidAdmission(error) => {
                write!(formatter, "invalid command admission: {error}")
            }
            Self::Orchestration(_) => formatter.write_str("lifecycle dispatch failed"),
        }
    }
}

impl std::error::Error for LifecycleDispatchError {}

#[derive(Debug)]
pub enum C1DispatchError {
    InvalidAdmission(CommandAdmissionRequestError),
    Orchestration(
        CommandOrchestrationError<CommandAdmissionError, C1LiveActionCommandExecutionError>,
    ),
}

impl C1DispatchError {
    pub fn to_application_error(&self) -> ApplicationError {
        match self {
            Self::InvalidAdmission(_) => invalid_processing_lease(),
            Self::Orchestration(CommandOrchestrationError::Admission(error)) => {
                error.to_application_error()
            }
            Self::Orchestration(CommandOrchestrationError::Execution(error)) => {
                error.to_application_error()
            }
            Self::Orchestration(_) => integrity_error(),
        }
    }
}

impl fmt::Display for C1DispatchError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidAdmission(error) => {
                write!(formatter, "invalid command admission: {error}")
            }
            Self::Orchestration(_) => formatter.write_str("C1 dispatch failed"),
        }
    }
}

impl std::error::Error for C1DispatchError {}

pub async fn dispatch_lifecycle_command(
    pool: &PgPool,
    dispatch: MatchCommandDispatch,
    command: &MatchLifecycleCommand,
) -> Result<
    CommandOrchestrationResult<
        crate::LifecycleCommandResult,
        football_domain::MatchLifecycleCommandError,
    >,
    LifecycleDispatchError,
> {
    let (request, context) = dispatch
        .request_and_context(
            command.command_id(),
            command.actor(),
            command.expected_version(),
        )
        .map_err(LifecycleDispatchError::InvalidAdmission)?;
    CommandOrchestrator::new(
        PostgresCommandAdmission::new(pool),
        PostgresLifecycleExecutor::new(pool, dispatch.now_utc),
    )
    .execute(&request, &context, command)
    .await
    .map_err(LifecycleDispatchError::Orchestration)
}

pub async fn dispatch_c1_command(
    pool: &PgPool,
    dispatch: MatchCommandDispatch,
    command: &football_domain::C1LiveActionCommand,
) -> Result<
    CommandOrchestrationResult<
        crate::C1LiveActionResult,
        football_domain::C1LiveActionCommandError,
    >,
    C1DispatchError,
> {
    let (request, context) = dispatch
        .request_and_context(
            command.command_id(),
            command.actor(),
            command.expected_version(),
        )
        .map_err(C1DispatchError::InvalidAdmission)?;
    CommandOrchestrator::new(
        PostgresCommandAdmission::new(pool),
        PostgresC1LiveActionExecutor::new(pool, dispatch.now_utc),
    )
    .execute(&request, &context, command)
    .await
    .map_err(C1DispatchError::Orchestration)
}

fn invalid_processing_lease() -> ApplicationError {
    ApplicationError::new(
        ErrorCode::InternalUnexpected,
        "the command processing lease could not be created",
    )
}

fn integrity_error() -> ApplicationError {
    ApplicationError::new(
        ErrorCode::InternalUnexpected,
        "the command orchestration identity is inconsistent",
    )
}
