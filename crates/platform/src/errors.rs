use std::{collections::BTreeMap, fmt};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ErrorCategory {
    Request,
    Authentication,
    Authorization,
    NotFound,
    Validation,
    Domain,
    Conflict,
    Idempotency,
    RateLimit,
    Dependency,
    Internal,
}

impl ErrorCategory {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Request => "request",
            Self::Authentication => "authentication",
            Self::Authorization => "authorization",
            Self::NotFound => "not_found",
            Self::Validation => "validation",
            Self::Domain => "domain",
            Self::Conflict => "conflict",
            Self::Idempotency => "idempotency",
            Self::RateLimit => "rate_limit",
            Self::Dependency => "dependency",
            Self::Internal => "internal",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ErrorCode {
    RequestInvalidJson,
    RequestBodyTooLarge,
    AuthenticationRequired,
    AuthenticationInvalidSession,
    AuthorizationForbidden,
    ResourceNotFound,
    ValidationRequiredField,
    ValidationInvalidValue,
    DomainInvalidTransition,
    DomainLineupDuplicatePlayer,
    DomainPlayerNotOnPitch,
    DomainMatchInterrupted,
    DomainCorrectionCycle,
    ConflictExpectedVersion,
    ConflictOperatorLease,
    ConflictUniqueConstraint,
    IdempotencyFingerprintMismatch,
    IdempotencyCommandInProgress,
    IdempotencyReconciliationRequired,
    DependencyDatabaseUnavailable,
    DependencyTimeout,
    RateLimitExceeded,
    InternalUnexpected,
}

impl ErrorCode {
    /// Parses only the stable persisted error-code vocabulary.
    pub fn try_from_str(value: &str) -> Option<Self> {
        match value {
            "request.invalid_json" => Some(Self::RequestInvalidJson),
            "request.body_too_large" => Some(Self::RequestBodyTooLarge),
            "authentication.required" => Some(Self::AuthenticationRequired),
            "authentication.invalid_session" => Some(Self::AuthenticationInvalidSession),
            "authorization.forbidden" => Some(Self::AuthorizationForbidden),
            "resource.not_found" => Some(Self::ResourceNotFound),
            "validation.required_field" => Some(Self::ValidationRequiredField),
            "validation.invalid_value" => Some(Self::ValidationInvalidValue),
            "domain.invalid_transition" => Some(Self::DomainInvalidTransition),
            "domain.lineup_duplicate_player" => Some(Self::DomainLineupDuplicatePlayer),
            "domain.player_not_on_pitch" => Some(Self::DomainPlayerNotOnPitch),
            "domain.match_interrupted" => Some(Self::DomainMatchInterrupted),
            "domain.correction_cycle" => Some(Self::DomainCorrectionCycle),
            "conflict.expected_version" => Some(Self::ConflictExpectedVersion),
            "conflict.operator_lease" => Some(Self::ConflictOperatorLease),
            "conflict.unique_constraint" => Some(Self::ConflictUniqueConstraint),
            "idempotency.fingerprint_mismatch" => Some(Self::IdempotencyFingerprintMismatch),
            "idempotency.command_in_progress" => Some(Self::IdempotencyCommandInProgress),
            "idempotency.reconciliation_required" => Some(Self::IdempotencyReconciliationRequired),
            "dependency.database_unavailable" => Some(Self::DependencyDatabaseUnavailable),
            "dependency.timeout" => Some(Self::DependencyTimeout),
            "rate_limit.exceeded" => Some(Self::RateLimitExceeded),
            "internal.unexpected" => Some(Self::InternalUnexpected),
            _ => None,
        }
    }

    pub const fn category(self) -> ErrorCategory {
        match self {
            Self::RequestInvalidJson | Self::RequestBodyTooLarge => ErrorCategory::Request,
            Self::AuthenticationRequired | Self::AuthenticationInvalidSession => {
                ErrorCategory::Authentication
            }
            Self::AuthorizationForbidden => ErrorCategory::Authorization,
            Self::ResourceNotFound => ErrorCategory::NotFound,
            Self::ValidationRequiredField | Self::ValidationInvalidValue => {
                ErrorCategory::Validation
            }
            Self::DomainInvalidTransition
            | Self::DomainLineupDuplicatePlayer
            | Self::DomainPlayerNotOnPitch
            | Self::DomainMatchInterrupted
            | Self::DomainCorrectionCycle => ErrorCategory::Domain,
            Self::ConflictExpectedVersion
            | Self::ConflictOperatorLease
            | Self::ConflictUniqueConstraint => ErrorCategory::Conflict,
            Self::IdempotencyFingerprintMismatch
            | Self::IdempotencyCommandInProgress
            | Self::IdempotencyReconciliationRequired => ErrorCategory::Idempotency,
            Self::DependencyDatabaseUnavailable | Self::DependencyTimeout => {
                ErrorCategory::Dependency
            }
            Self::RateLimitExceeded => ErrorCategory::RateLimit,
            Self::InternalUnexpected => ErrorCategory::Internal,
        }
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::RequestInvalidJson => "request.invalid_json",
            Self::RequestBodyTooLarge => "request.body_too_large",
            Self::AuthenticationRequired => "authentication.required",
            Self::AuthenticationInvalidSession => "authentication.invalid_session",
            Self::AuthorizationForbidden => "authorization.forbidden",
            Self::ResourceNotFound => "resource.not_found",
            Self::ValidationRequiredField => "validation.required_field",
            Self::ValidationInvalidValue => "validation.invalid_value",
            Self::DomainInvalidTransition => "domain.invalid_transition",
            Self::DomainLineupDuplicatePlayer => "domain.lineup_duplicate_player",
            Self::DomainPlayerNotOnPitch => "domain.player_not_on_pitch",
            Self::DomainMatchInterrupted => "domain.match_interrupted",
            Self::DomainCorrectionCycle => "domain.correction_cycle",
            Self::ConflictExpectedVersion => "conflict.expected_version",
            Self::ConflictOperatorLease => "conflict.operator_lease",
            Self::ConflictUniqueConstraint => "conflict.unique_constraint",
            Self::IdempotencyFingerprintMismatch => "idempotency.fingerprint_mismatch",
            Self::IdempotencyCommandInProgress => "idempotency.command_in_progress",
            Self::IdempotencyReconciliationRequired => "idempotency.reconciliation_required",
            Self::DependencyDatabaseUnavailable => "dependency.database_unavailable",
            Self::DependencyTimeout => "dependency.timeout",
            Self::RateLimitExceeded => "rate_limit.exceeded",
            Self::InternalUnexpected => "internal.unexpected",
        }
    }

    pub const fn retryable(self) -> bool {
        matches!(
            self,
            Self::DependencyDatabaseUnavailable | Self::DependencyTimeout | Self::RateLimitExceeded
        )
    }
}

/// Stable, transport-neutral application failure.
///
/// Transport adapters map this value to HTTP or CLI output. Details are kept
/// as safe string metadata for now; the later contract adapter owns JSON
/// rendering and must not add sensitive payloads.
#[derive(Clone, PartialEq, Eq)]
pub struct ApplicationError {
    code: ErrorCode,
    message: String,
    details: BTreeMap<String, String>,
}

impl ApplicationError {
    pub fn new(code: ErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            details: BTreeMap::new(),
        }
    }

    pub fn with_detail(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.details.insert(key.into(), value.into());
        self
    }

    pub const fn code(&self) -> ErrorCode {
        self.code
    }

    pub const fn category(&self) -> ErrorCategory {
        self.code.category()
    }

    pub const fn retryable(&self) -> bool {
        self.code.retryable()
    }

    pub fn message(&self) -> &str {
        &self.message
    }

    pub fn details(&self) -> &BTreeMap<String, String> {
        &self.details
    }
}

impl fmt::Debug for ApplicationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ApplicationError")
            .field("code", &self.code.as_str())
            .field("category", &self.category().as_str())
            .field("retryable", &self.retryable())
            .field("detail_count", &self.details.len())
            .finish()
    }
}

impl fmt::Display for ApplicationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}: {}", self.code.as_str(), self.message)
    }
}

impl std::error::Error for ApplicationError {}

#[cfg(test)]
mod tests {
    use super::{ApplicationError, ErrorCategory, ErrorCode};

    #[test]
    fn exposes_stable_code_category_and_retryability() {
        let error = ApplicationError::new(
            ErrorCode::ConflictExpectedVersion,
            "the match changed before this command was applied",
        );

        assert_eq!(error.code().as_str(), "conflict.expected_version");
        assert_eq!(error.category(), ErrorCategory::Conflict);
        assert!(!error.retryable());
        assert_eq!(
            error.to_string(),
            "conflict.expected_version: the match changed before this command was applied"
        );
    }

    #[test]
    fn parses_only_the_stable_error_code_vocabulary() {
        assert_eq!(
            ErrorCode::try_from_str("conflict.expected_version"),
            Some(ErrorCode::ConflictExpectedVersion)
        );
        assert_eq!(
            ErrorCode::try_from_str("domain.invalid_transition"),
            Some(ErrorCode::DomainInvalidTransition)
        );
        assert_eq!(
            ErrorCode::try_from_str("idempotency.reconciliation_required"),
            Some(ErrorCode::IdempotencyReconciliationRequired)
        );
        assert_eq!(ErrorCode::try_from_str("unknown.code"), None);
    }

    #[test]
    fn dependency_and_rate_limit_codes_are_retryable() {
        assert!(ErrorCode::DependencyDatabaseUnavailable.retryable());
        assert!(ErrorCode::DependencyTimeout.retryable());
        assert!(ErrorCode::RateLimitExceeded.retryable());
        assert!(!ErrorCode::InternalUnexpected.retryable());
    }

    #[test]
    fn idempotency_fingerprint_conflict_is_stable_and_final() {
        let error = ApplicationError::new(
            ErrorCode::IdempotencyFingerprintMismatch,
            "the command ID is already bound to a different request",
        );

        assert_eq!(error.code().as_str(), "idempotency.fingerprint_mismatch");
        assert_eq!(error.category(), ErrorCategory::Idempotency);
        assert!(!error.retryable());
    }

    #[test]
    fn active_command_lease_conflict_is_stable_and_final() {
        let error = ApplicationError::new(
            ErrorCode::IdempotencyCommandInProgress,
            "the command is already being processed",
        );

        assert_eq!(error.code().as_str(), "idempotency.command_in_progress");
        assert_eq!(error.category(), ErrorCategory::Idempotency);
        assert!(!error.retryable());
    }

    #[test]
    fn reconciliation_required_is_stable_and_final_until_state_is_resolved() {
        let error = ApplicationError::new(
            ErrorCode::IdempotencyReconciliationRequired,
            "the command state must be reconciled before another execution attempt",
        );

        assert_eq!(error.code().as_str(), "idempotency.reconciliation_required");
        assert_eq!(error.category(), ErrorCategory::Idempotency);
        assert!(!error.retryable());
    }

    #[test]
    fn idempotency_error_debug_output_does_not_render_fingerprint_metadata() {
        let error = ApplicationError::new(
            ErrorCode::IdempotencyFingerprintMismatch,
            "the command ID is already bound to a different request",
        )
        .with_detail("client_command_id", "019fe310-3bc5-7cf1-b3d2-0728c077a930")
        .with_detail(
            "request_fingerprint",
            "11223344556677889900aabbccddeeff00112233445566778899aabbccddeeff",
        );

        let debug_output = format!("{error:?}");

        assert!(!debug_output.contains("019fe310-3bc5-7cf1-b3d2-0728c077a930"));
        assert!(!debug_output.contains("11223344556677889900aabbccddeeff"));
        assert!(debug_output.contains("idempotency.fingerprint_mismatch"));
    }

    #[test]
    fn debug_output_does_not_render_detail_values() {
        let error = ApplicationError::new(ErrorCode::InternalUnexpected, "unexpected failure")
            .with_detail("internal_token", "do-not-render");

        let debug_output = format!("{error:?}");

        assert!(!debug_output.contains("do-not-render"));
        assert!(debug_output.contains("internal.unexpected"));
        assert!(debug_output.contains("detail_count"));
    }
}
