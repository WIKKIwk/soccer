use football_domain::{
    ActorIdentity, AggregateVersion, CommandId, CorrectionReason, EventId, EvidenceReference,
    MatchId, UtcInstant,
};
use serde_json::{Map, Value};
use sqlx::{Error as SqlxError, PgPool, Postgres, Transaction, types::Json};
use std::fmt;
use uuid::Uuid;

/// Result state recorded by the first command-audit slice.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommandAuditOutcome {
    Accepted,
    Rejected,
}

/// Typed audit details for a critical correction or verification decision.
///
/// The canonical event remains the source of match state. This value only
/// records the business decision context in the command audit row; it never
/// replaces the immutable event or interprets an evidence reference as a
/// fetch instruction.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CriticalDecisionAudit {
    VoidEvent {
        voids_event_id: EventId,
        reason: CorrectionReason,
        evidence_reference: Option<EvidenceReference>,
    },
    VerifyMatch {
        evidence_reference: Option<EvidenceReference>,
    },
    ReopenMatch {
        reason: CorrectionReason,
        evidence_reference: Option<EvidenceReference>,
    },
}

impl CriticalDecisionAudit {
    pub fn void_event(
        voids_event_id: EventId,
        reason: CorrectionReason,
        evidence_reference: Option<EvidenceReference>,
    ) -> Self {
        Self::VoidEvent {
            voids_event_id,
            reason,
            evidence_reference,
        }
    }

    pub fn verify_match(evidence_reference: Option<EvidenceReference>) -> Self {
        Self::VerifyMatch { evidence_reference }
    }

    pub fn reopen_match(
        reason: CorrectionReason,
        evidence_reference: Option<EvidenceReference>,
    ) -> Self {
        Self::ReopenMatch {
            reason,
            evidence_reference,
        }
    }

    pub const fn action(&self) -> &'static str {
        match self {
            Self::VoidEvent { .. } => "VoidEvent",
            Self::VerifyMatch { .. } => "VerifyMatch",
            Self::ReopenMatch { .. } => "ReopenMatch",
        }
    }

    pub fn details(&self) -> Value {
        let mut details = Map::new();
        details.insert(
            "decision".to_owned(),
            Value::String(self.decision_name().to_owned()),
        );

        match self {
            Self::VoidEvent {
                voids_event_id,
                reason,
                ..
            } => {
                details.insert(
                    "voids_event_id".to_owned(),
                    Value::String(voids_event_id.to_string()),
                );
                details.insert("reason".to_owned(), Value::String(reason.to_string()));
            }
            Self::VerifyMatch { .. } => {}
            Self::ReopenMatch { reason, .. } => {
                details.insert("reason".to_owned(), Value::String(reason.to_string()));
            }
        }

        let evidence_reference = match self {
            Self::VoidEvent {
                evidence_reference, ..
            }
            | Self::VerifyMatch { evidence_reference }
            | Self::ReopenMatch {
                evidence_reference, ..
            } => evidence_reference,
        };
        if let Some(evidence_reference) = evidence_reference {
            details.insert(
                "evidence_reference".to_owned(),
                Value::String(evidence_reference.to_string()),
            );
        }

        Value::Object(details)
    }

    const fn decision_name(&self) -> &'static str {
        match self {
            Self::VoidEvent { .. } => "void_event",
            Self::VerifyMatch { .. } => "verify_match",
            Self::ReopenMatch { .. } => "reopen_match",
        }
    }
}

impl CommandAuditOutcome {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Accepted => "accepted",
            Self::Rejected => "rejected",
        }
    }
}

/// Safe audit data for one logical command.
///
/// The command ID is the first-slice audit identity. This intentionally does
/// not carry request payloads or fingerprints; audit records retain business
/// outcome metadata, while the command table owns idempotency data.
#[derive(Debug, Clone, PartialEq)]
pub struct CommandAuditEntry {
    command_id: CommandId,
    actor: ActorIdentity,
    aggregate_id: MatchId,
    action: String,
    outcome: CommandAuditOutcome,
    aggregate_version: AggregateVersion,
    event_count: u64,
    event_id: Option<EventId>,
    rejection_code: Option<String>,
    details: Value,
    created_at: UtcInstant,
}

impl CommandAuditEntry {
    #[allow(clippy::too_many_arguments)]
    pub fn accepted(
        command_id: CommandId,
        actor: ActorIdentity,
        aggregate_id: MatchId,
        action: impl Into<String>,
        aggregate_version: AggregateVersion,
        event_count: u64,
        event_id: EventId,
        created_at: UtcInstant,
    ) -> Result<Self, CommandAuditEntryError> {
        Self::accepted_with_details(
            command_id,
            actor,
            aggregate_id,
            action,
            aggregate_version,
            event_count,
            event_id,
            Value::Object(serde_json::Map::new()),
            created_at,
        )
    }

    /// Builds an accepted audit entry for a correction or verification
    /// decision while preserving its typed business details.
    pub fn accepted_decision(
        command_id: CommandId,
        actor: ActorIdentity,
        aggregate_id: MatchId,
        decision: &CriticalDecisionAudit,
        aggregate_version: AggregateVersion,
        event_id: EventId,
        created_at: UtcInstant,
    ) -> Result<Self, CommandAuditEntryError> {
        Self::accepted_with_details(
            command_id,
            actor,
            aggregate_id,
            decision.action(),
            aggregate_version,
            1,
            event_id,
            decision.details(),
            created_at,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn accepted_with_details(
        command_id: CommandId,
        actor: ActorIdentity,
        aggregate_id: MatchId,
        action: impl Into<String>,
        aggregate_version: AggregateVersion,
        event_count: u64,
        event_id: EventId,
        details: Value,
        created_at: UtcInstant,
    ) -> Result<Self, CommandAuditEntryError> {
        Self::try_new(
            command_id,
            actor,
            aggregate_id,
            action,
            CommandAuditOutcome::Accepted,
            aggregate_version,
            event_count,
            Some(event_id),
            None,
            details,
            created_at,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub fn rejected(
        command_id: CommandId,
        actor: ActorIdentity,
        aggregate_id: MatchId,
        action: impl Into<String>,
        aggregate_version: AggregateVersion,
        rejection_code: impl Into<String>,
        details: Value,
        created_at: UtcInstant,
    ) -> Result<Self, CommandAuditEntryError> {
        Self::try_new(
            command_id,
            actor,
            aggregate_id,
            action,
            CommandAuditOutcome::Rejected,
            aggregate_version,
            0,
            None,
            Some(rejection_code.into()),
            details,
            created_at,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn try_new(
        command_id: CommandId,
        actor: ActorIdentity,
        aggregate_id: MatchId,
        action: impl Into<String>,
        outcome: CommandAuditOutcome,
        aggregate_version: AggregateVersion,
        event_count: u64,
        event_id: Option<EventId>,
        rejection_code: Option<String>,
        details: Value,
        created_at: UtcInstant,
    ) -> Result<Self, CommandAuditEntryError> {
        let action = action.into();
        if action.trim().is_empty() {
            return Err(CommandAuditEntryError::ActionBlank);
        }
        if !details.is_object() {
            return Err(CommandAuditEntryError::DetailsNotObject);
        }

        match outcome {
            CommandAuditOutcome::Accepted if rejection_code.is_some() => {
                return Err(CommandAuditEntryError::AcceptedHasRejectionCode);
            }
            CommandAuditOutcome::Rejected
                if event_id.is_some()
                    || event_count != 0
                    || rejection_code
                        .as_deref()
                        .is_none_or(|value| value.trim().is_empty()) =>
            {
                return Err(CommandAuditEntryError::RejectedShapeInvalid);
            }
            _ => {}
        }

        Ok(Self {
            command_id,
            actor,
            aggregate_id,
            action,
            outcome,
            aggregate_version,
            event_count,
            event_id,
            rejection_code,
            details,
            created_at,
        })
    }

    pub const fn command_id(&self) -> CommandId {
        self.command_id
    }

    pub const fn actor(&self) -> ActorIdentity {
        self.actor
    }

    pub const fn aggregate_id(&self) -> MatchId {
        self.aggregate_id
    }

    pub fn action(&self) -> &str {
        &self.action
    }

    pub const fn outcome(&self) -> CommandAuditOutcome {
        self.outcome
    }

    pub const fn aggregate_version(&self) -> AggregateVersion {
        self.aggregate_version
    }

    pub const fn event_count(&self) -> u64 {
        self.event_count
    }

    pub const fn event_id(&self) -> Option<EventId> {
        self.event_id
    }

    pub fn rejection_code(&self) -> Option<&str> {
        self.rejection_code.as_deref()
    }

    pub fn details(&self) -> &Value {
        &self.details
    }

    pub const fn created_at(&self) -> UtcInstant {
        self.created_at
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommandAuditEntryError {
    ActionBlank,
    DetailsNotObject,
    AcceptedHasRejectionCode,
    RejectedShapeInvalid,
}

impl fmt::Display for CommandAuditEntryError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ActionBlank => formatter.write_str("audit action must not be blank"),
            Self::DetailsNotObject => formatter.write_str("audit details must be a JSON object"),
            Self::AcceptedHasRejectionCode => {
                formatter.write_str("accepted audit entry cannot have a rejection code")
            }
            Self::RejectedShapeInvalid => {
                formatter.write_str("rejected audit entry has an invalid shape")
            }
        }
    }
}

impl std::error::Error for CommandAuditEntryError {}

#[derive(Debug)]
pub enum CommandAuditRepositoryError {
    Database(SqlxError),
    CannotPersistAggregateVersion { command_id: CommandId, value: u64 },
    CannotPersistEventCount { command_id: CommandId, value: u64 },
}

impl fmt::Display for CommandAuditRepositoryError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Database(error) => write!(
                formatter,
                "command audit database operation failed: {error}"
            ),
            Self::CannotPersistAggregateVersion { command_id, value } => write!(
                formatter,
                "command {command_id} aggregate version {value} does not fit PostgreSQL BIGINT"
            ),
            Self::CannotPersistEventCount { command_id, value } => write!(
                formatter,
                "command {command_id} event count {value} does not fit PostgreSQL BIGINT"
            ),
        }
    }
}

impl std::error::Error for CommandAuditRepositoryError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Database(error) => Some(error),
            Self::CannotPersistAggregateVersion { .. } | Self::CannotPersistEventCount { .. } => {
                None
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct StoredCommandAudit {
    pub command_id: CommandId,
    pub actor_id: Uuid,
    pub aggregate_id: MatchId,
    pub action: String,
    pub outcome: String,
    pub aggregate_version: u64,
    pub event_count: u64,
    pub event_id: Option<EventId>,
    pub rejection_code: Option<String>,
    pub details: Value,
    pub created_at: UtcInstant,
}

#[derive(Debug, sqlx::FromRow)]
struct StoredCommandAuditRow {
    command_id: Uuid,
    actor_id: Uuid,
    aggregate_id: Uuid,
    action: String,
    outcome: String,
    aggregate_version: i64,
    event_count: i64,
    event_id: Option<Uuid>,
    rejection_code: Option<String>,
    details: Json<Value>,
    created_at: time::OffsetDateTime,
}

impl TryFrom<StoredCommandAuditRow> for StoredCommandAudit {
    type Error = CommandAuditRepositoryError;

    fn try_from(row: StoredCommandAuditRow) -> Result<Self, Self::Error> {
        let aggregate_version = u64::try_from(row.aggregate_version).map_err(|_| {
            CommandAuditRepositoryError::CannotPersistAggregateVersion {
                command_id: CommandId::from_uuid(row.command_id),
                value: row.aggregate_version as u64,
            }
        })?;
        let event_count = u64::try_from(row.event_count).map_err(|_| {
            CommandAuditRepositoryError::CannotPersistEventCount {
                command_id: CommandId::from_uuid(row.command_id),
                value: row.event_count as u64,
            }
        })?;

        Ok(Self {
            command_id: CommandId::from_uuid(row.command_id),
            actor_id: row.actor_id,
            aggregate_id: MatchId::from_uuid(row.aggregate_id),
            action: row.action,
            outcome: row.outcome,
            aggregate_version,
            event_count,
            event_id: row.event_id.map(EventId::from_uuid),
            rejection_code: row.rejection_code,
            details: row.details.0,
            created_at: UtcInstant::from_offset_datetime(row.created_at),
        })
    }
}

pub async fn persist_command_audit(
    transaction: &mut Transaction<'_, Postgres>,
    entry: &CommandAuditEntry,
) -> Result<(), CommandAuditRepositoryError> {
    let aggregate_version = i64::try_from(entry.aggregate_version().value()).map_err(|_| {
        CommandAuditRepositoryError::CannotPersistAggregateVersion {
            command_id: entry.command_id(),
            value: entry.aggregate_version().value(),
        }
    })?;
    let event_count = i64::try_from(entry.event_count()).map_err(|_| {
        CommandAuditRepositoryError::CannotPersistEventCount {
            command_id: entry.command_id(),
            value: entry.event_count(),
        }
    })?;

    sqlx::query(
        r#"
        INSERT INTO command_audit (
            command_id,
            actor_id,
            aggregate_id,
            action,
            outcome,
            aggregate_version,
            event_count,
            event_id,
            rejection_code,
            details,
            created_at
        )
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)
        "#,
    )
    .bind(entry.command_id().as_uuid())
    .bind(entry.actor().admin_user_id().as_uuid())
    .bind(entry.aggregate_id().as_uuid())
    .bind(entry.action())
    .bind(entry.outcome().as_str())
    .bind(aggregate_version)
    .bind(event_count)
    .bind(entry.event_id().map(EventId::as_uuid))
    .bind(entry.rejection_code())
    .bind(Json(entry.details().clone()))
    .bind(entry.created_at().as_offset_datetime())
    .execute(&mut **transaction)
    .await
    .map_err(CommandAuditRepositoryError::Database)?;

    Ok(())
}

pub async fn load_command_audit(
    pool: &PgPool,
    command_id: CommandId,
) -> Result<StoredCommandAudit, CommandAuditRepositoryError> {
    let row = sqlx::query_as::<_, StoredCommandAuditRow>(
        r#"
        SELECT command_id,
               actor_id,
               aggregate_id,
               action,
               outcome,
               aggregate_version,
               event_count,
               event_id,
               rejection_code,
               details,
               created_at
        FROM command_audit
        WHERE command_id = $1
        "#,
    )
    .bind(command_id.as_uuid())
    .fetch_one(pool)
    .await
    .map_err(CommandAuditRepositoryError::Database)?;

    row.try_into()
}

#[cfg(test)]
mod tests {
    use super::{CommandAuditEntry, CommandAuditOutcome, CriticalDecisionAudit};
    use football_domain::{
        ActorIdentity, AdminUserId, AggregateVersion, CommandId, CorrectionReason, EventId,
        EvidenceReference, MatchId, UtcInstant,
    };
    use serde_json::json;
    use uuid::Uuid;

    fn uuid(value: &str) -> Uuid {
        Uuid::parse_str(value).expect("audit fixture UUID should be valid")
    }

    fn command_id() -> CommandId {
        CommandId::from_uuid(uuid("019fea00-3bc5-7cf0-b3d2-0728c077a930"))
    }

    fn actor() -> ActorIdentity {
        ActorIdentity::administrator(AdminUserId::from_uuid(uuid(
            "019fea00-3bc5-7cf1-b3d2-0728c077a930",
        )))
    }

    fn match_id() -> MatchId {
        MatchId::from_uuid(uuid("019fea00-3bc5-7cf2-b3d2-0728c077a930"))
    }

    fn event_id() -> EventId {
        EventId::from_uuid(uuid("019fea00-3bc5-7cf3-b3d2-0728c077a930"))
    }

    fn evidence_reference() -> EvidenceReference {
        EvidenceReference::try_new("PFL protocol ref 2026/001")
            .expect("evidence reference fixture should be valid")
    }

    fn now() -> UtcInstant {
        UtcInstant::from_unix_timestamp(8_000).expect("audit fixture time should be valid")
    }

    #[test]
    fn accepted_entry_preserves_business_correlation_without_payload() {
        let entry = CommandAuditEntry::accepted(
            command_id(),
            actor(),
            match_id(),
            "RecordGoal",
            AggregateVersion::from_value(1),
            1,
            football_domain::EventId::from_uuid(uuid("019fea00-3bc5-7cf3-b3d2-0728c077a930")),
            now(),
        )
        .expect("accepted audit entry should be valid");

        assert_eq!(entry.outcome(), CommandAuditOutcome::Accepted);
        assert_eq!(entry.action(), "RecordGoal");
        assert_eq!(entry.details(), &json!({}));
        assert_eq!(entry.rejection_code(), None);
    }

    #[test]
    fn rejected_entry_requires_zero_events_and_object_details() {
        let entry = CommandAuditEntry::rejected(
            command_id(),
            actor(),
            match_id(),
            "RecordGoal",
            AggregateVersion::from_value(1),
            "conflict.expected_version",
            json!({"expected_version": "0", "actual_version": "1"}),
            now(),
        )
        .expect("rejected audit entry should be valid");

        assert_eq!(entry.outcome(), CommandAuditOutcome::Rejected);
        assert_eq!(entry.event_count(), 0);
        assert_eq!(entry.event_id(), None);
        assert_eq!(entry.rejection_code(), Some("conflict.expected_version"));
    }

    #[test]
    fn void_decision_details_preserve_target_reason_and_evidence() {
        let decision = CriticalDecisionAudit::void_event(
            event_id(),
            CorrectionReason::try_new("official protocol removed the provisional goal")
                .expect("decision reason should be valid"),
            Some(evidence_reference()),
        );

        assert_eq!(decision.action(), "VoidEvent");
        assert_eq!(
            decision.details(),
            json!({
                "decision": "void_event",
                "evidence_reference": "PFL protocol ref 2026/001",
                "reason": "official protocol removed the provisional goal",
                "voids_event_id": event_id().to_string(),
            })
        );
    }

    #[test]
    fn verification_and_reopen_decisions_have_stable_audit_shapes() {
        let verification = CriticalDecisionAudit::verify_match(None);
        assert_eq!(verification.action(), "VerifyMatch");
        assert_eq!(verification.details(), json!({"decision": "verify_match"}));

        let reopen = CriticalDecisionAudit::reopen_match(
            CorrectionReason::try_new("disciplinary correction requires review")
                .expect("reopen reason should be valid"),
            Some(evidence_reference()),
        );
        assert_eq!(reopen.action(), "ReopenMatch");
        assert_eq!(
            reopen.details(),
            json!({
                "decision": "reopen_match",
                "evidence_reference": "PFL protocol ref 2026/001",
                "reason": "disciplinary correction requires review",
            })
        );
    }

    #[test]
    fn accepted_decision_entry_records_one_canonical_event_and_typed_details() {
        let decision = CriticalDecisionAudit::verify_match(Some(evidence_reference()));
        let entry = CommandAuditEntry::accepted_decision(
            command_id(),
            actor(),
            match_id(),
            &decision,
            AggregateVersion::from_value(3),
            event_id(),
            now(),
        )
        .expect("accepted decision audit should be valid");

        assert_eq!(entry.action(), "VerifyMatch");
        assert_eq!(entry.outcome(), CommandAuditOutcome::Accepted);
        assert_eq!(entry.aggregate_version(), AggregateVersion::from_value(3));
        assert_eq!(entry.event_count(), 1);
        assert_eq!(entry.event_id(), Some(event_id()));
        assert_eq!(
            entry.details(),
            &json!({
                "decision": "verify_match",
                "evidence_reference": "PFL protocol ref 2026/001",
            })
        );
    }
}
