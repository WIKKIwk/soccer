use football_domain::{
    ActorIdentity, AggregateVersion, ClientCommandId, CommandId, CommandOutcome,
    CommandResultEnvelope, CommandStatus, MatchId, RequestFingerprint, UtcInstant,
};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::fmt;
use std::future::Future;
use uuid::Uuid;

/// Identity and fingerprint that admission uses to make one logical command
/// idempotent.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LogicalCommandIdentity {
    command_id: CommandId,
    client_command_id: ClientCommandId,
    request_fingerprint: RequestFingerprint,
}

impl LogicalCommandIdentity {
    pub const fn new(
        command_id: CommandId,
        client_command_id: ClientCommandId,
        request_fingerprint: RequestFingerprint,
    ) -> Self {
        Self {
            command_id,
            client_command_id,
            request_fingerprint,
        }
    }

    pub const fn command_id(self) -> CommandId {
        self.command_id
    }

    pub const fn client_command_id(self) -> ClientCommandId {
        self.client_command_id
    }

    pub const fn request_fingerprint(self) -> RequestFingerprint {
        self.request_fingerprint
    }
}

/// Result of the short command-admission boundary.
#[derive(Debug, Clone, PartialEq)]
pub enum PersistedCommandResult {
    Accepted {
        command_id: CommandId,
        aggregate_id: MatchId,
        completed_at: UtcInstant,
        final_aggregate_version: AggregateVersion,
        event_count: u64,
        result_payload: Value,
    },
    Rejected {
        command_id: CommandId,
        aggregate_id: MatchId,
        completed_at: UtcInstant,
        aggregate_version: AggregateVersion,
        rejection_code: String,
        rejection_details: Value,
    },
}

impl PersistedCommandResult {
    pub const fn command_id(&self) -> CommandId {
        match self {
            Self::Accepted { command_id, .. } | Self::Rejected { command_id, .. } => *command_id,
        }
    }

    pub const fn status(&self) -> CommandStatus {
        match self {
            Self::Accepted { .. } => CommandStatus::Accepted,
            Self::Rejected { .. } => CommandStatus::Rejected,
        }
    }
}

/// Converts a typed domain result into the opaque terminal shape used by the
/// command store. The caller supplies both payload encoders so this boundary
/// does not invent command-specific JSON or rejection codes, and it performs
/// no database write.
pub fn into_persisted_command_result<R, E, AcceptedEncoder, RejectedEncoder>(
    envelope: CommandResultEnvelope<R, E>,
    completed_at: UtcInstant,
    encode_accepted: AcceptedEncoder,
    encode_rejected: RejectedEncoder,
) -> PersistedCommandResult
where
    AcceptedEncoder: FnOnce(R) -> Value,
    RejectedEncoder: FnOnce(E) -> (String, Value),
{
    let command_id = envelope.command_id();
    let aggregate_id = envelope.match_id();

    match envelope.into_outcome() {
        CommandOutcome::Accepted {
            final_aggregate_version,
            event_count,
            result,
        } => PersistedCommandResult::Accepted {
            command_id,
            aggregate_id,
            completed_at,
            final_aggregate_version,
            event_count,
            result_payload: encode_accepted(result),
        },
        CommandOutcome::Rejected {
            aggregate_version,
            error,
        } => {
            let (rejection_code, rejection_details) = encode_rejected(error);
            PersistedCommandResult::Rejected {
                command_id,
                aggregate_id,
                completed_at,
                aggregate_version,
                rejection_code,
                rejection_details,
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum CommandAdmissionResult {
    Claimed { command_id: CommandId },
    ExistingTerminal { result: PersistedCommandResult },
}

/// Application-owned port for establishing command ownership before domain
/// execution. Infrastructure adapters implement this port; the application
/// remains independent of SQLx and PostgreSQL details.
pub trait CommandAdmissionPort {
    type Error;

    fn admit<'a>(
        &'a self,
        request: &'a CommandAdmissionRequest,
    ) -> impl Future<Output = Result<CommandAdmissionResult, Self::Error>> + Send + 'a;
}

/// Application-owned port for the domain transaction after admission. The
/// adapter must perform load, execute, canonical-event/state writes, audit,
/// outbox writes, and terminal-result persistence atomically.
pub trait CommandExecutionPort<Command> {
    type Result;
    type DomainError;
    type Error;

    fn execute_and_commit<'a>(
        &'a self,
        context: &'a CommandExecutionContext,
        command: &'a Command,
    ) -> impl Future<
        Output = Result<CommandResultEnvelope<Self::Result, Self::DomainError>, Self::Error>,
    > + Send
    + 'a;
}

/// Immutable execution metadata passed after admission into the domain
/// transaction boundary.
///
/// The processing lease is copied from the admission request and must be
/// propagated unchanged. Infrastructure uses it to prove that the same
/// worker still owns the command before committing a terminal result.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CommandExecutionContext {
    command_id: CommandId,
    actor: ActorIdentity,
    expected_version: Option<AggregateVersion>,
    processing_owner: Uuid,
    processing_expires_at: football_domain::UtcInstant,
}

impl CommandExecutionContext {
    pub const fn new(
        command_id: CommandId,
        actor: ActorIdentity,
        expected_version: Option<AggregateVersion>,
        processing_owner: Uuid,
        processing_expires_at: football_domain::UtcInstant,
    ) -> Self {
        Self {
            command_id,
            actor,
            expected_version,
            processing_owner,
            processing_expires_at,
        }
    }

    pub const fn from_admission_request(
        request: CommandAdmissionRequest,
        actor: ActorIdentity,
        expected_version: Option<AggregateVersion>,
    ) -> Self {
        Self::new(
            request.identity().command_id(),
            actor,
            expected_version,
            request.processing_owner(),
            request.processing_expires_at(),
        )
    }

    pub const fn command_id(self) -> CommandId {
        self.command_id
    }

    pub const fn actor(self) -> ActorIdentity {
        self.actor
    }

    pub const fn expected_version(self) -> Option<AggregateVersion> {
        self.expected_version
    }

    pub const fn processing_owner(self) -> Uuid {
        self.processing_owner
    }

    pub const fn processing_expires_at(self) -> football_domain::UtcInstant {
        self.processing_expires_at
    }
}

/// Result of one orchestration attempt. A terminal admission result is
/// returned unchanged and never reaches the execution port.
#[derive(Debug)]
pub enum CommandOrchestrationResult<R, E> {
    ExistingTerminal { result: PersistedCommandResult },
    Executed(CommandResultEnvelope<R, E>),
}

#[derive(Debug)]
pub enum CommandOrchestrationError<AdmissionError, ExecutionError> {
    Admission(AdmissionError),
    AdmissionResultIdentityMismatch {
        expected: CommandId,
        actual: CommandId,
    },
    Execution(ExecutionError),
    ExecutionContextIdentityMismatch {
        expected: CommandId,
        actual: CommandId,
    },
    ExecutionContextLeaseMismatch,
    CommandResultIdentityMismatch {
        expected: CommandId,
        actual: CommandId,
    },
}

/// Coordinates command admission and the subsequent atomic domain execution
/// boundary. It intentionally does not own SQL transactions or domain rules.
pub struct CommandOrchestrator<Admission, Execution> {
    admission: Admission,
    execution: Execution,
}

impl<Admission, Execution> CommandOrchestrator<Admission, Execution> {
    pub const fn new(admission: Admission, execution: Execution) -> Self {
        Self {
            admission,
            execution,
        }
    }
}

impl<Admission, Execution> CommandOrchestrator<Admission, Execution>
where
    Admission: CommandAdmissionPort,
{
    pub async fn execute<Command>(
        &self,
        request: &CommandAdmissionRequest,
        context: &CommandExecutionContext,
        command: &Command,
    ) -> Result<
        CommandOrchestrationResult<Execution::Result, Execution::DomainError>,
        CommandOrchestrationError<Admission::Error, Execution::Error>,
    >
    where
        Execution: CommandExecutionPort<Command>,
    {
        let expected_command_id = request.identity().command_id();
        if context.command_id() != expected_command_id {
            return Err(
                CommandOrchestrationError::ExecutionContextIdentityMismatch {
                    expected: expected_command_id,
                    actual: context.command_id(),
                },
            );
        }

        match self
            .admission
            .admit(request)
            .await
            .map_err(CommandOrchestrationError::Admission)?
        {
            CommandAdmissionResult::ExistingTerminal { result } => {
                let actual = result.command_id();
                if actual != expected_command_id {
                    return Err(CommandOrchestrationError::AdmissionResultIdentityMismatch {
                        expected: expected_command_id,
                        actual,
                    });
                }

                Ok(CommandOrchestrationResult::ExistingTerminal { result })
            }
            CommandAdmissionResult::Claimed { command_id } => {
                if command_id != expected_command_id {
                    return Err(CommandOrchestrationError::AdmissionResultIdentityMismatch {
                        expected: expected_command_id,
                        actual: command_id,
                    });
                }
                if context.processing_owner() != request.processing_owner()
                    || context.processing_expires_at() != request.processing_expires_at()
                {
                    return Err(CommandOrchestrationError::ExecutionContextLeaseMismatch);
                }

                let result = self
                    .execution
                    .execute_and_commit(context, command)
                    .await
                    .map_err(CommandOrchestrationError::Execution)?;
                if result.command_id() != expected_command_id {
                    return Err(CommandOrchestrationError::CommandResultIdentityMismatch {
                        expected: expected_command_id,
                        actual: result.command_id(),
                    });
                }

                Ok(CommandOrchestrationResult::Executed(result))
            }
        }
    }
}

/// Calculates the fingerprint from bytes that have already been canonicalized
/// by the command boundary.
pub fn fingerprint_canonical_bytes(canonical_bytes: &[u8]) -> RequestFingerprint {
    let digest = Sha256::digest(canonical_bytes);
    let mut bytes = [0_u8; 32];
    bytes.copy_from_slice(&digest);
    RequestFingerprint::from_bytes(bytes)
}

/// Inputs required by the short command-admission transaction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CommandAdmissionRequest {
    identity: LogicalCommandIdentity,
    processing_owner: Uuid,
    now_utc: football_domain::UtcInstant,
    processing_expires_at: football_domain::UtcInstant,
}

impl CommandAdmissionRequest {
    pub fn try_new(
        identity: LogicalCommandIdentity,
        processing_owner: Uuid,
        now_utc: football_domain::UtcInstant,
        processing_expires_at: football_domain::UtcInstant,
    ) -> Result<Self, CommandAdmissionRequestError> {
        if processing_expires_at.as_offset_datetime() <= now_utc.as_offset_datetime() {
            return Err(CommandAdmissionRequestError::LeaseExpiryNotAfterNow);
        }

        Ok(Self {
            identity,
            processing_owner,
            now_utc,
            processing_expires_at,
        })
    }

    pub const fn identity(self) -> LogicalCommandIdentity {
        self.identity
    }

    pub const fn processing_owner(self) -> Uuid {
        self.processing_owner
    }

    pub const fn now_utc(self) -> football_domain::UtcInstant {
        self.now_utc
    }

    pub const fn processing_expires_at(self) -> football_domain::UtcInstant {
        self.processing_expires_at
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommandAdmissionRequestError {
    LeaseExpiryNotAfterNow,
}

impl fmt::Display for CommandAdmissionRequestError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::LeaseExpiryNotAfterNow => {
                formatter.write_str("processing lease expiry must be after current UTC time")
            }
        }
    }
}

impl std::error::Error for CommandAdmissionRequestError {}

#[cfg(test)]
mod tests {
    use std::future::{Future, ready};
    use std::sync::atomic::{AtomicUsize, Ordering};

    use serde_json::json;
    use uuid::Uuid;

    use football_domain::{
        ActorIdentity, AdminUserId, AggregateVersion, ClientCommandId, CommandId,
        CommandResultEnvelope, MatchId, RequestFingerprint, UtcInstant,
    };

    use super::{
        CommandAdmissionPort, CommandAdmissionRequest, CommandAdmissionRequestError,
        CommandAdmissionResult, CommandExecutionContext, CommandExecutionPort,
        CommandOrchestrationError, CommandOrchestrationResult, CommandOrchestrator,
        LogicalCommandIdentity, PersistedCommandResult, fingerprint_canonical_bytes,
        into_persisted_command_result,
    };

    fn uuid(value: &str) -> Uuid {
        Uuid::parse_str(value).expect("command UUID should be valid")
    }

    fn client_command_id() -> ClientCommandId {
        ClientCommandId::try_from_uuid(uuid("019fe2d0-3bc5-7cf0-b3d2-0728c077a930"))
            .expect("fixture command ID should be UUIDv7")
    }

    #[test]
    fn hashes_canonical_bytes_with_sha256() {
        let fingerprint = fingerprint_canonical_bytes(b"");

        assert_eq!(
            fingerprint.to_string(),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        assert_eq!(fingerprint, fingerprint_canonical_bytes(b""));
        assert_ne!(fingerprint, fingerprint_canonical_bytes(b"different"));
    }

    #[test]
    fn encodes_typed_command_outcomes_without_inventing_codes_or_payload_shape() {
        let command_id = CommandId::from_uuid(uuid("019fe2d0-3bc5-7cf0-b3d2-0728c077a930"));
        let completed_at = UtcInstant::from_unix_timestamp(1_005).unwrap();
        let accepted = into_persisted_command_result(
            CommandResultEnvelope::accepted(
                command_id,
                match_id(),
                AggregateVersion::from_value(2),
                2,
                "accepted-payload",
            ),
            completed_at,
            |payload| json!({"payload": payload}),
            |error: &'static str| (error.to_owned(), json!({"unused": true})),
        );
        assert!(matches!(
            accepted,
            PersistedCommandResult::Accepted {
                command_id: actual,
                aggregate_id,
                completed_at: actual_completed_at,
                final_aggregate_version,
                event_count: 2,
                result_payload,
            } if actual == command_id
                && aggregate_id == match_id()
                && actual_completed_at == completed_at
                && final_aggregate_version == AggregateVersion::from_value(2)
                && result_payload == json!({"payload": "accepted-payload"})
        ));

        let rejected = into_persisted_command_result(
            CommandResultEnvelope::<(), _>::rejected(
                command_id,
                match_id(),
                AggregateVersion::from_value(2),
                "domain.invalid_transition",
            ),
            completed_at,
            |_: ()| json!({"unused": true}),
            |error| (error.to_owned(), json!({"state": "half_time"})),
        );
        assert!(matches!(
            rejected,
            PersistedCommandResult::Rejected {
                command_id: actual,
                rejection_code,
                rejection_details,
                aggregate_version,
                ..
            } if actual == command_id
                && rejection_code == "domain.invalid_transition"
                && rejection_details == json!({"state": "half_time"})
                && aggregate_version == AggregateVersion::from_value(2)
        ));
    }

    #[test]
    fn logical_command_identity_keeps_internal_and_client_identity_distinct() {
        let command_id = CommandId::from_uuid(uuid("019fe2d0-3bc5-7cf1-b3d2-0728c077a930"));
        let fingerprint = RequestFingerprint::from_bytes([0x11; 32]);
        let identity = LogicalCommandIdentity::new(command_id, client_command_id(), fingerprint);

        assert_eq!(identity.command_id(), command_id);
        assert_eq!(identity.client_command_id(), client_command_id());
        assert_eq!(identity.request_fingerprint(), fingerprint);
    }

    #[test]
    fn admission_request_requires_a_future_processing_lease() {
        let identity = LogicalCommandIdentity::new(
            CommandId::from_uuid(uuid("019fe2d0-3bc5-7cf1-b3d2-0728c077a930")),
            client_command_id(),
            RequestFingerprint::from_bytes([0x11; 32]),
        );
        let now = UtcInstant::from_unix_timestamp(1_000).unwrap();

        assert_eq!(
            CommandAdmissionRequest::try_new(
                identity,
                uuid("019fe2d0-3bc5-7cf2-b3d2-0728c077a930"),
                now,
                now,
            )
            .unwrap_err(),
            CommandAdmissionRequestError::LeaseExpiryNotAfterNow
        );

        let request = CommandAdmissionRequest::try_new(
            identity,
            uuid("019fe2d0-3bc5-7cf2-b3d2-0728c077a930"),
            now,
            UtcInstant::from_unix_timestamp(1_030).unwrap(),
        )
        .unwrap();
        assert_eq!(request.identity(), identity);
    }

    #[derive(Debug, Clone, Copy)]
    struct FixtureCommand {
        value: &'static str,
    }

    #[derive(Debug)]
    struct FixedAdmission {
        result: CommandAdmissionResult,
    }

    impl CommandAdmissionPort for FixedAdmission {
        type Error = &'static str;

        fn admit<'a>(
            &'a self,
            _request: &'a CommandAdmissionRequest,
        ) -> impl Future<Output = Result<CommandAdmissionResult, Self::Error>> + Send + 'a {
            ready(Ok(self.result.clone()))
        }
    }

    #[derive(Debug, Clone, Copy)]
    struct FailingAdmission;

    impl CommandAdmissionPort for FailingAdmission {
        type Error = &'static str;

        fn admit<'a>(
            &'a self,
            _request: &'a CommandAdmissionRequest,
        ) -> impl Future<Output = Result<CommandAdmissionResult, Self::Error>> + Send + 'a {
            ready(Err("admission-failed"))
        }
    }

    struct FixedExecution {
        result: CommandResultEnvelope<&'static str, &'static str>,
        calls: AtomicUsize,
    }

    impl CommandExecutionPort<FixtureCommand> for FixedExecution {
        type Result = &'static str;
        type DomainError = &'static str;
        type Error = &'static str;

        fn execute_and_commit<'a>(
            &'a self,
            context: &'a CommandExecutionContext,
            command: &'a FixtureCommand,
        ) -> impl Future<
            Output = Result<CommandResultEnvelope<Self::Result, Self::DomainError>, Self::Error>,
        > + Send
        + 'a {
            self.calls.fetch_add(1, Ordering::SeqCst);
            assert_eq!(command.value, "fixture-command");
            assert_eq!(context.actor(), actor());
            assert_eq!(
                context.expected_version(),
                Some(AggregateVersion::initial())
            );
            assert_eq!(
                context.processing_owner(),
                uuid("019fe2d0-3bc5-7cf2-b3d2-0728c077a930")
            );
            assert_eq!(
                context.processing_expires_at(),
                UtcInstant::from_unix_timestamp(1_030).unwrap()
            );
            ready(Ok(self.result.clone()))
        }
    }

    struct FailingExecution {
        calls: AtomicUsize,
    }

    impl CommandExecutionPort<FixtureCommand> for FailingExecution {
        type Result = &'static str;
        type DomainError = &'static str;
        type Error = &'static str;

        fn execute_and_commit<'a>(
            &'a self,
            _context: &'a CommandExecutionContext,
            _command: &'a FixtureCommand,
        ) -> impl Future<
            Output = Result<CommandResultEnvelope<Self::Result, Self::DomainError>, Self::Error>,
        > + Send
        + 'a {
            self.calls.fetch_add(1, Ordering::SeqCst);
            ready(Err("execution-failed"))
        }
    }

    fn request_for(command_id: CommandId) -> CommandAdmissionRequest {
        let identity = LogicalCommandIdentity::new(
            command_id,
            client_command_id(),
            RequestFingerprint::from_bytes([0x11; 32]),
        );
        CommandAdmissionRequest::try_new(
            identity,
            uuid("019fe2d0-3bc5-7cf2-b3d2-0728c077a930"),
            UtcInstant::from_unix_timestamp(1_000).unwrap(),
            UtcInstant::from_unix_timestamp(1_030).unwrap(),
        )
        .unwrap()
    }

    fn match_id() -> MatchId {
        MatchId::from_uuid(uuid("019fe2d0-3bc5-7cf3-b3d2-0728c077a930"))
    }

    fn actor() -> ActorIdentity {
        ActorIdentity::administrator(AdminUserId::from_uuid(uuid(
            "019fe2d0-3bc5-7cf8-b3d2-0728c077a930",
        )))
    }

    fn context(command_id: CommandId) -> CommandExecutionContext {
        CommandExecutionContext::new(
            command_id,
            actor(),
            Some(AggregateVersion::initial()),
            uuid("019fe2d0-3bc5-7cf2-b3d2-0728c077a930"),
            UtcInstant::from_unix_timestamp(1_030).unwrap(),
        )
    }

    #[test]
    fn execution_context_can_copy_the_processing_lease_from_admission() {
        let command_id = CommandId::from_uuid(uuid("019fe2d0-3bc5-7cf4-b3d2-0728c077a930"));
        let request = request_for(command_id);
        let context = CommandExecutionContext::from_admission_request(
            request,
            actor(),
            Some(AggregateVersion::initial()),
        );

        assert_eq!(context.command_id(), command_id);
        assert_eq!(context.processing_owner(), request.processing_owner());
        assert_eq!(
            context.processing_expires_at(),
            request.processing_expires_at()
        );
    }

    #[tokio::test]
    async fn terminal_admission_does_not_execute_the_domain_command() {
        let command_id = CommandId::from_uuid(uuid("019fe2d0-3bc5-7cf4-b3d2-0728c077a930"));
        let execution = FixedExecution {
            result: CommandResultEnvelope::accepted(
                command_id,
                match_id(),
                AggregateVersion::initial(),
                0,
                "accepted",
            ),
            calls: AtomicUsize::new(0),
        };
        let orchestrator = CommandOrchestrator::new(
            FixedAdmission {
                result: CommandAdmissionResult::ExistingTerminal {
                    result: PersistedCommandResult::Accepted {
                        command_id,
                        aggregate_id: match_id(),
                        completed_at: UtcInstant::from_unix_timestamp(1_005).unwrap(),
                        final_aggregate_version: AggregateVersion::initial(),
                        event_count: 0,
                        result_payload: json!({"result": "accepted"}),
                    },
                },
            },
            execution,
        );

        let result = orchestrator
            .execute(
                &request_for(command_id),
                &context(command_id),
                &FixtureCommand {
                    value: "fixture-command",
                },
            )
            .await
            .unwrap();

        assert!(matches!(
            result,
            CommandOrchestrationResult::ExistingTerminal {
                result: PersistedCommandResult::Accepted {
                    command_id: actual,
                    result_payload,
                    ..
                },
            } if actual == command_id && result_payload == json!({"result": "accepted"})
        ));
        assert_eq!(orchestrator.execution.calls.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn terminal_admission_rejects_a_result_for_a_different_command() {
        let command_id = CommandId::from_uuid(uuid("019fe2d0-3bc5-7cf4-b3d2-0728c077a930"));
        let actual_command_id = CommandId::from_uuid(uuid("019fe2d0-3bc5-7cf5-b3d2-0728c077a930"));
        let execution = FixedExecution {
            result: CommandResultEnvelope::accepted(
                command_id,
                match_id(),
                AggregateVersion::initial(),
                0,
                "accepted",
            ),
            calls: AtomicUsize::new(0),
        };
        let orchestrator = CommandOrchestrator::new(
            FixedAdmission {
                result: CommandAdmissionResult::ExistingTerminal {
                    result: PersistedCommandResult::Rejected {
                        command_id: actual_command_id,
                        aggregate_id: match_id(),
                        completed_at: UtcInstant::from_unix_timestamp(1_005).unwrap(),
                        aggregate_version: AggregateVersion::initial(),
                        rejection_code: "domain.invalid_transition".to_owned(),
                        rejection_details: json!({"state": "pre_match"}),
                    },
                },
            },
            execution,
        );

        let error = orchestrator
            .execute(
                &request_for(command_id),
                &context(command_id),
                &FixtureCommand {
                    value: "fixture-command",
                },
            )
            .await
            .unwrap_err();

        assert!(matches!(
            error,
            CommandOrchestrationError::AdmissionResultIdentityMismatch { expected, actual }
                if expected == command_id && actual == actual_command_id
        ));
        assert_eq!(orchestrator.execution.calls.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn propagates_admission_failure_without_executing() {
        let command_id = CommandId::from_uuid(uuid("019fe2d0-3bc5-7cf4-b3d2-0728c077a930"));
        let execution = FixedExecution {
            result: CommandResultEnvelope::accepted(
                command_id,
                match_id(),
                AggregateVersion::initial(),
                0,
                "accepted",
            ),
            calls: AtomicUsize::new(0),
        };
        let orchestrator = CommandOrchestrator::new(FailingAdmission, execution);

        let error = orchestrator
            .execute(
                &request_for(command_id),
                &context(command_id),
                &FixtureCommand {
                    value: "fixture-command",
                },
            )
            .await
            .unwrap_err();

        assert!(matches!(
            error,
            CommandOrchestrationError::Admission("admission-failed")
        ));
        assert_eq!(orchestrator.execution.calls.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn claimed_admission_executes_and_checks_result_identity() {
        let command_id = CommandId::from_uuid(uuid("019fe2d0-3bc5-7cf5-b3d2-0728c077a930"));
        let execution = FixedExecution {
            result: CommandResultEnvelope::accepted(
                command_id,
                match_id(),
                AggregateVersion::initial().try_next().unwrap(),
                1,
                "accepted",
            ),
            calls: AtomicUsize::new(0),
        };
        let orchestrator = CommandOrchestrator::new(
            FixedAdmission {
                result: CommandAdmissionResult::Claimed { command_id },
            },
            execution,
        );

        let result = orchestrator
            .execute(
                &request_for(command_id),
                &context(command_id),
                &FixtureCommand {
                    value: "fixture-command",
                },
            )
            .await
            .unwrap();

        assert!(matches!(
            result,
            CommandOrchestrationResult::Executed(envelope)
                if envelope.command_id() == command_id
        ));
        assert_eq!(orchestrator.execution.calls.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn propagates_execution_failure_after_a_successful_claim() {
        let command_id = CommandId::from_uuid(uuid("019fe2d0-3bc5-7cf5-b3d2-0728c077a930"));
        let execution = FailingExecution {
            calls: AtomicUsize::new(0),
        };
        let orchestrator = CommandOrchestrator::new(
            FixedAdmission {
                result: CommandAdmissionResult::Claimed { command_id },
            },
            execution,
        );

        let error = orchestrator
            .execute(
                &request_for(command_id),
                &context(command_id),
                &FixtureCommand {
                    value: "fixture-command",
                },
            )
            .await
            .unwrap_err();

        assert!(matches!(
            error,
            CommandOrchestrationError::Execution("execution-failed")
        ));
        assert_eq!(orchestrator.execution.calls.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn rejects_an_execution_context_for_a_different_command() {
        let command_id = CommandId::from_uuid(uuid("019fe2d0-3bc5-7cf9-b3d2-0728c077a930"));
        let other_command_id = CommandId::from_uuid(uuid("019fe2d0-3bc5-7cfa-b3d2-0728c077a930"));
        let execution = FixedExecution {
            result: CommandResultEnvelope::accepted(
                command_id,
                match_id(),
                AggregateVersion::initial(),
                0,
                "accepted",
            ),
            calls: AtomicUsize::new(0),
        };
        let orchestrator = CommandOrchestrator::new(
            FixedAdmission {
                result: CommandAdmissionResult::Claimed { command_id },
            },
            execution,
        );

        let error = orchestrator
            .execute(
                &request_for(command_id),
                &context(other_command_id),
                &FixtureCommand {
                    value: "fixture-command",
                },
            )
            .await
            .unwrap_err();

        assert!(matches!(
            error,
            CommandOrchestrationError::ExecutionContextIdentityMismatch {
                expected,
                actual,
            } if expected == command_id && actual == other_command_id
        ));
        assert_eq!(orchestrator.execution.calls.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn rejects_an_execution_context_with_a_different_processing_lease() {
        let command_id = CommandId::from_uuid(uuid("019fe2d0-3bc5-7cf9-b3d2-0728c077a930"));
        let execution = FixedExecution {
            result: CommandResultEnvelope::accepted(
                command_id,
                match_id(),
                AggregateVersion::initial(),
                0,
                "accepted",
            ),
            calls: AtomicUsize::new(0),
        };
        let orchestrator = CommandOrchestrator::new(
            FixedAdmission {
                result: CommandAdmissionResult::Claimed { command_id },
            },
            execution,
        );
        let mismatched_context = CommandExecutionContext::new(
            command_id,
            actor(),
            Some(AggregateVersion::initial()),
            uuid("019fe2d0-3bc5-7cfa-b3d2-0728c077a930"),
            UtcInstant::from_unix_timestamp(1_030).unwrap(),
        );

        let error = orchestrator
            .execute(
                &request_for(command_id),
                &mismatched_context,
                &FixtureCommand {
                    value: "fixture-command",
                },
            )
            .await
            .unwrap_err();

        assert!(matches!(
            error,
            CommandOrchestrationError::ExecutionContextLeaseMismatch
        ));
        assert_eq!(orchestrator.execution.calls.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn rejects_a_claimed_admission_for_a_different_command() {
        let command_id = CommandId::from_uuid(uuid("019fe2d0-3bc5-7cfb-b3d2-0728c077a930"));
        let other_command_id = CommandId::from_uuid(uuid("019fe2d0-3bc5-7cfc-b3d2-0728c077a930"));
        let execution = FixedExecution {
            result: CommandResultEnvelope::accepted(
                command_id,
                match_id(),
                AggregateVersion::initial(),
                0,
                "accepted",
            ),
            calls: AtomicUsize::new(0),
        };
        let orchestrator = CommandOrchestrator::new(
            FixedAdmission {
                result: CommandAdmissionResult::Claimed {
                    command_id: other_command_id,
                },
            },
            execution,
        );

        let error = orchestrator
            .execute(
                &request_for(command_id),
                &context(command_id),
                &FixtureCommand {
                    value: "fixture-command",
                },
            )
            .await
            .unwrap_err();

        assert!(matches!(
            error,
            CommandOrchestrationError::AdmissionResultIdentityMismatch {
                expected,
                actual,
            } if expected == command_id && actual == other_command_id
        ));
        assert_eq!(orchestrator.execution.calls.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn rejects_a_domain_result_for_a_different_command() {
        let command_id = CommandId::from_uuid(uuid("019fe2d0-3bc5-7cf6-b3d2-0728c077a930"));
        let other_command_id = CommandId::from_uuid(uuid("019fe2d0-3bc5-7cf7-b3d2-0728c077a930"));
        let execution = FixedExecution {
            result: CommandResultEnvelope::accepted(
                other_command_id,
                match_id(),
                AggregateVersion::initial(),
                0,
                "accepted",
            ),
            calls: AtomicUsize::new(0),
        };
        let orchestrator = CommandOrchestrator::new(
            FixedAdmission {
                result: CommandAdmissionResult::Claimed { command_id },
            },
            execution,
        );

        let error = orchestrator
            .execute(
                &request_for(command_id),
                &context(command_id),
                &FixtureCommand {
                    value: "fixture-command",
                },
            )
            .await
            .unwrap_err();

        assert!(matches!(
            error,
            CommandOrchestrationError::CommandResultIdentityMismatch {
                expected,
                actual,
            } if expected == command_id && actual == other_command_id
        ));
        assert_eq!(orchestrator.execution.calls.load(Ordering::SeqCst), 1);
    }
}
