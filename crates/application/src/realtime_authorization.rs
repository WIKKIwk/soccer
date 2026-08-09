use football_contracts::RealtimeTopic;
use football_domain::AdminUserId;
use std::fmt;

/// The authenticated or public principal requesting a topic subscription.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RealtimePrincipal {
    PublicClient,
    MatchDataOperator { admin_user_id: AdminUserId },
    MatchDataSupervisor { admin_user_id: AdminUserId },
}

/// The control operation being authorized.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RealtimeSubscriptionOperation {
    Subscribe,
    Unsubscribe,
}

/// Authoritative access facts loaded by the application boundary.
///
/// The policy does not guess these facts from a topic ID. Public visibility,
/// operator assignment, and supervisor scope must come from their owning
/// application/authentication queries.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RealtimeTopicAuthorizationFacts {
    pub public_readable: bool,
    pub operator_assigned: bool,
    pub supervisor_authorized: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RealtimeTopicAuthorizationError {
    PublicTopicNotReadable { topic: RealtimeTopic },
    OperatorNotAssigned { topic: RealtimeTopic },
    SupervisorNotAuthorized { topic: RealtimeTopic },
}

impl fmt::Display for RealtimeTopicAuthorizationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::PublicTopicNotReadable { topic } => {
                write!(formatter, "public client cannot subscribe to topic {topic}")
            }
            Self::OperatorNotAssigned { topic } => write!(
                formatter,
                "match data operator is not assigned to topic {topic}"
            ),
            Self::SupervisorNotAuthorized { topic } => write!(
                formatter,
                "match data supervisor is not authorized for topic {topic}"
            ),
        }
    }
}

impl std::error::Error for RealtimeTopicAuthorizationError {}

/// Authorizes a topic control operation without depending on WebSocket state.
///
/// Unsubscribe is always allowed because it only removes the requesting
/// connection's local subscription and reveals no topic data. Subscribe is
/// fail-closed and requires the principal-specific access fact.
pub fn authorize_realtime_subscription(
    principal: RealtimePrincipal,
    operation: RealtimeSubscriptionOperation,
    topic: RealtimeTopic,
    facts: RealtimeTopicAuthorizationFacts,
) -> Result<(), RealtimeTopicAuthorizationError> {
    if operation == RealtimeSubscriptionOperation::Unsubscribe {
        return Ok(());
    }

    match principal {
        RealtimePrincipal::PublicClient if facts.public_readable => Ok(()),
        RealtimePrincipal::PublicClient => {
            Err(RealtimeTopicAuthorizationError::PublicTopicNotReadable { topic })
        }
        RealtimePrincipal::MatchDataOperator { .. } if facts.operator_assigned => Ok(()),
        RealtimePrincipal::MatchDataOperator { .. } => {
            Err(RealtimeTopicAuthorizationError::OperatorNotAssigned { topic })
        }
        RealtimePrincipal::MatchDataSupervisor { .. } if facts.supervisor_authorized => Ok(()),
        RealtimePrincipal::MatchDataSupervisor { .. } => {
            Err(RealtimeTopicAuthorizationError::SupervisorNotAuthorized { topic })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        RealtimePrincipal, RealtimeSubscriptionOperation, RealtimeTopicAuthorizationError,
        RealtimeTopicAuthorizationFacts, authorize_realtime_subscription,
    };
    use football_contracts::RealtimeTopic;
    use football_domain::AdminUserId;
    use uuid::Uuid;

    fn topic() -> RealtimeTopic {
        RealtimeTopic::match_topic(
            Uuid::parse_str("019ff300-3bc5-7cf0-b3d2-0728c077a930")
                .expect("authorization fixture UUID should be valid"),
        )
    }

    fn standings_topic() -> RealtimeTopic {
        RealtimeTopic::standings_topic(
            Uuid::parse_str("019ff302-3bc5-7cf0-b3d2-0728c077a930")
                .expect("authorization standings fixture UUID should be valid"),
        )
    }

    fn admin_id() -> AdminUserId {
        AdminUserId::from_uuid(
            Uuid::parse_str("019ff301-3bc5-7cf0-b3d2-0728c077a930")
                .expect("authorization fixture UUID should be valid"),
        )
    }

    #[test]
    fn public_clients_can_subscribe_only_to_publicly_readable_topics() {
        let public_facts = RealtimeTopicAuthorizationFacts {
            public_readable: true,
            operator_assigned: false,
            supervisor_authorized: false,
        };
        assert_eq!(
            authorize_realtime_subscription(
                RealtimePrincipal::PublicClient,
                RealtimeSubscriptionOperation::Subscribe,
                topic(),
                public_facts,
            ),
            Ok(())
        );

        let restricted_facts = RealtimeTopicAuthorizationFacts {
            public_readable: false,
            ..public_facts
        };
        assert_eq!(
            authorize_realtime_subscription(
                RealtimePrincipal::PublicClient,
                RealtimeSubscriptionOperation::Subscribe,
                topic(),
                restricted_facts,
            ),
            Err(RealtimeTopicAuthorizationError::PublicTopicNotReadable { topic: topic() })
        );
    }

    #[test]
    fn operator_and_supervisor_access_require_their_own_scope_fact() {
        let restricted_facts = RealtimeTopicAuthorizationFacts {
            public_readable: false,
            operator_assigned: true,
            supervisor_authorized: false,
        };
        assert_eq!(
            authorize_realtime_subscription(
                RealtimePrincipal::MatchDataOperator {
                    admin_user_id: admin_id(),
                },
                RealtimeSubscriptionOperation::Subscribe,
                topic(),
                restricted_facts,
            ),
            Ok(())
        );
        assert_eq!(
            authorize_realtime_subscription(
                RealtimePrincipal::MatchDataSupervisor {
                    admin_user_id: admin_id(),
                },
                RealtimeSubscriptionOperation::Subscribe,
                topic(),
                restricted_facts,
            ),
            Err(RealtimeTopicAuthorizationError::SupervisorNotAuthorized { topic: topic() })
        );
    }

    #[test]
    fn unsubscribe_is_safe_after_access_is_revoked() {
        assert_eq!(
            authorize_realtime_subscription(
                RealtimePrincipal::PublicClient,
                RealtimeSubscriptionOperation::Unsubscribe,
                topic(),
                RealtimeTopicAuthorizationFacts {
                    public_readable: false,
                    operator_assigned: false,
                    supervisor_authorized: false,
                },
            ),
            Ok(())
        );
    }

    #[test]
    fn public_standings_subscriptions_use_the_same_authoritative_readability_fact() {
        assert_eq!(
            authorize_realtime_subscription(
                RealtimePrincipal::PublicClient,
                RealtimeSubscriptionOperation::Subscribe,
                standings_topic(),
                RealtimeTopicAuthorizationFacts {
                    public_readable: true,
                    operator_assigned: false,
                    supervisor_authorized: false,
                },
            ),
            Ok(())
        );
        assert_eq!(
            authorize_realtime_subscription(
                RealtimePrincipal::PublicClient,
                RealtimeSubscriptionOperation::Subscribe,
                standings_topic(),
                RealtimeTopicAuthorizationFacts {
                    public_readable: false,
                    operator_assigned: false,
                    supervisor_authorized: false,
                },
            ),
            Err(RealtimeTopicAuthorizationError::PublicTopicNotReadable {
                topic: standings_topic()
            })
        );
    }
}
