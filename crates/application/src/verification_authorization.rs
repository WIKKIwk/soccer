use football_domain::AdminUserId;
use std::fmt;

/// Authenticated role claim used by the match-verification command boundary.
///
/// The authentication/identity adapter is responsible for constructing this
/// value from an authenticated session and authoritative role data. The
/// policy never infers a role from a user ID.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MatchVerificationPrincipal {
    MatchDataOperator { admin_user_id: AdminUserId },
    MatchDataSupervisor { admin_user_id: AdminUserId },
}

/// Verification operation whose resource/action permission must be checked.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MatchVerificationOperation {
    Verify,
    Reopen,
}

impl MatchVerificationOperation {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Verify => "verify_match",
            Self::Reopen => "reopen_match",
        }
    }
}

impl fmt::Display for MatchVerificationOperation {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

/// Authoritative access facts loaded by the identity/access boundary.
///
/// Capabilities are operation-specific so granting reopen does not silently
/// grant verify, and a missing fact remains a denial rather than an inferred
/// permission.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MatchVerificationAuthorizationFacts {
    pub can_verify: bool,
    pub can_reopen: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MatchVerificationAuthorizationError {
    SupervisorRequired {
        operation: MatchVerificationOperation,
    },
    SupervisorNotAuthorized {
        operation: MatchVerificationOperation,
    },
}

impl fmt::Display for MatchVerificationAuthorizationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::SupervisorRequired { operation } => write!(
                formatter,
                "match data supervisor role is required for {operation}"
            ),
            Self::SupervisorNotAuthorized { operation } => write!(
                formatter,
                "match data supervisor is not authorized for {operation}"
            ),
        }
    }
}

impl std::error::Error for MatchVerificationAuthorizationError {}

/// Enforces the supervisor-only boundary for future verification commands.
///
/// This is a pure application policy. It does not mutate a match, create a
/// canonical event, or decide whether the supplied role/capability facts are
/// true; those facts must come from the authenticated identity/access query.
pub fn authorize_match_verification(
    principal: MatchVerificationPrincipal,
    operation: MatchVerificationOperation,
    facts: MatchVerificationAuthorizationFacts,
) -> Result<(), MatchVerificationAuthorizationError> {
    if matches!(
        principal,
        MatchVerificationPrincipal::MatchDataOperator { .. }
    ) {
        return Err(MatchVerificationAuthorizationError::SupervisorRequired { operation });
    }

    let allowed = match operation {
        MatchVerificationOperation::Verify => facts.can_verify,
        MatchVerificationOperation::Reopen => facts.can_reopen,
    };

    if allowed {
        Ok(())
    } else {
        Err(MatchVerificationAuthorizationError::SupervisorNotAuthorized { operation })
    }
}

#[cfg(test)]
mod tests {
    use super::{
        MatchVerificationAuthorizationError, MatchVerificationAuthorizationFacts,
        MatchVerificationOperation, MatchVerificationPrincipal, authorize_match_verification,
    };
    use football_domain::AdminUserId;
    use std::cell::Cell;
    use uuid::Uuid;

    fn admin_id() -> AdminUserId {
        AdminUserId::from_uuid(
            Uuid::parse_str("019ff400-3bc5-7cf0-b3d2-0728c077a930")
                .expect("authorization fixture UUID should be valid"),
        )
    }

    fn supervisor() -> MatchVerificationPrincipal {
        MatchVerificationPrincipal::MatchDataSupervisor {
            admin_user_id: admin_id(),
        }
    }

    fn operator() -> MatchVerificationPrincipal {
        MatchVerificationPrincipal::MatchDataOperator {
            admin_user_id: admin_id(),
        }
    }

    fn run_authorized_mutation<F>(
        principal: MatchVerificationPrincipal,
        operation: MatchVerificationOperation,
        facts: MatchVerificationAuthorizationFacts,
        mutation: F,
    ) -> Result<(), MatchVerificationAuthorizationError>
    where
        F: FnOnce(),
    {
        authorize_match_verification(principal, operation, facts)?;
        mutation();
        Ok(())
    }

    #[test]
    fn supervisor_can_verify_when_the_verify_capability_is_authoritative() {
        assert_eq!(
            authorize_match_verification(
                supervisor(),
                MatchVerificationOperation::Verify,
                MatchVerificationAuthorizationFacts {
                    can_verify: true,
                    can_reopen: false,
                },
            ),
            Ok(())
        );
    }

    #[test]
    fn verify_and_reopen_capabilities_are_checked_independently() {
        assert_eq!(
            authorize_match_verification(
                supervisor(),
                MatchVerificationOperation::Reopen,
                MatchVerificationAuthorizationFacts {
                    can_verify: true,
                    can_reopen: false,
                },
            ),
            Err(
                MatchVerificationAuthorizationError::SupervisorNotAuthorized {
                    operation: MatchVerificationOperation::Reopen,
                }
            )
        );
        assert_eq!(
            authorize_match_verification(
                supervisor(),
                MatchVerificationOperation::Reopen,
                MatchVerificationAuthorizationFacts {
                    can_verify: false,
                    can_reopen: true,
                },
            ),
            Ok(())
        );
    }

    #[test]
    fn operator_cannot_verify_or_reopen_even_when_capability_facts_are_true() {
        let facts = MatchVerificationAuthorizationFacts {
            can_verify: true,
            can_reopen: true,
        };

        assert_eq!(
            authorize_match_verification(operator(), MatchVerificationOperation::Verify, facts),
            Err(MatchVerificationAuthorizationError::SupervisorRequired {
                operation: MatchVerificationOperation::Verify,
            })
        );
        assert_eq!(
            authorize_match_verification(operator(), MatchVerificationOperation::Reopen, facts),
            Err(MatchVerificationAuthorizationError::SupervisorRequired {
                operation: MatchVerificationOperation::Reopen,
            })
        );
    }

    #[test]
    fn missing_supervisor_capability_fails_closed_and_uses_stable_operation_names() {
        assert_eq!(MatchVerificationOperation::Verify.as_str(), "verify_match");
        assert_eq!(
            MatchVerificationOperation::Reopen.to_string(),
            "reopen_match"
        );
        assert_eq!(
            authorize_match_verification(
                supervisor(),
                MatchVerificationOperation::Verify,
                MatchVerificationAuthorizationFacts {
                    can_verify: false,
                    can_reopen: false,
                },
            )
            .unwrap_err()
            .to_string(),
            "match data supervisor is not authorized for verify_match"
        );
    }

    #[test]
    fn unauthorized_operator_workflows_never_reach_match_mutation() {
        let mutation_calls = Cell::new(0);
        let facts = MatchVerificationAuthorizationFacts {
            can_verify: true,
            can_reopen: true,
        };

        for operation in [
            MatchVerificationOperation::Verify,
            MatchVerificationOperation::Reopen,
        ] {
            let result = run_authorized_mutation(operator(), operation, facts, || {
                mutation_calls.set(mutation_calls.get() + 1)
            });

            assert_eq!(
                result,
                Err(MatchVerificationAuthorizationError::SupervisorRequired { operation })
            );
        }

        assert_eq!(
            mutation_calls.get(),
            0,
            "an unauthorized operator must not reach a match mutation"
        );
    }

    #[test]
    fn missing_supervisor_capability_stops_only_the_denied_workflow() {
        let mutation_calls = Cell::new(0);
        let facts = MatchVerificationAuthorizationFacts {
            can_verify: false,
            can_reopen: true,
        };

        let verify_result = run_authorized_mutation(
            supervisor(),
            MatchVerificationOperation::Verify,
            facts,
            || mutation_calls.set(mutation_calls.get() + 1),
        );
        assert_eq!(
            verify_result,
            Err(
                MatchVerificationAuthorizationError::SupervisorNotAuthorized {
                    operation: MatchVerificationOperation::Verify,
                }
            )
        );

        let reopen_result = run_authorized_mutation(
            supervisor(),
            MatchVerificationOperation::Reopen,
            facts,
            || mutation_calls.set(mutation_calls.get() + 1),
        );
        assert_eq!(reopen_result, Ok(()));
        assert_eq!(
            mutation_calls.get(),
            1,
            "only the specifically authorized workflow may reach mutation"
        );
    }
}
