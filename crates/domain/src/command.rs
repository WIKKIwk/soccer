use std::fmt;
use uuid::{Uuid, Version};

/// Caller-provided idempotency identity for one logical command.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ClientCommandId(Uuid);

impl ClientCommandId {
    pub fn try_from_uuid(value: Uuid) -> Result<Self, ClientCommandIdError> {
        if value.get_version() != Some(Version::SortRand) {
            return Err(ClientCommandIdError::NotUuidV7);
        }

        Ok(Self(value))
    }

    pub const fn as_uuid(self) -> Uuid {
        self.0
    }
}

impl fmt::Display for ClientCommandId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClientCommandIdError {
    NotUuidV7,
}

impl fmt::Display for ClientCommandIdError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotUuidV7 => formatter.write_str("client command ID must be UUIDv7"),
        }
    }
}

impl std::error::Error for ClientCommandIdError {}

/// Persisted lifecycle status of a logical command.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CommandStatus {
    Received,
    Processing,
    Accepted,
    Rejected,
    FailedRetryable,
}

impl CommandStatus {
    /// Returns the stable lowercase vocabulary persisted by the command store.
    pub const fn as_persisted_str(self) -> &'static str {
        match self {
            Self::Received => "received",
            Self::Processing => "processing",
            Self::Accepted => "accepted",
            Self::Rejected => "rejected",
            Self::FailedRetryable => "failed_retryable",
        }
    }

    /// Parses only the command-store status vocabulary. Unknown values are
    /// rejected so infrastructure adapters cannot silently guess a lifecycle
    /// state from corrupted persistence.
    pub fn try_from_persisted_str(value: &str) -> Option<Self> {
        match value {
            "received" => Some(Self::Received),
            "processing" => Some(Self::Processing),
            "accepted" => Some(Self::Accepted),
            "rejected" => Some(Self::Rejected),
            "failed_retryable" => Some(Self::FailedRetryable),
            _ => None,
        }
    }

    pub const fn is_terminal(self) -> bool {
        matches!(self, Self::Accepted | Self::Rejected)
    }

    pub const fn is_recoverable(self) -> bool {
        matches!(
            self,
            Self::Received | Self::Processing | Self::FailedRetryable
        )
    }

    pub const fn can_transition_to(self, next: Self) -> bool {
        matches!(
            (self, next),
            (Self::Received, Self::Processing)
                | (Self::Processing, Self::Accepted)
                | (Self::Processing, Self::Rejected)
                | (Self::Processing, Self::FailedRetryable)
                | (Self::FailedRetryable, Self::Processing)
        )
    }

    pub fn transition_to(self, next: Self) -> Result<Self, CommandStatusTransitionError> {
        if self.can_transition_to(next) {
            Ok(next)
        } else {
            Err(CommandStatusTransitionError {
                from: self,
                to: next,
            })
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CommandStatusTransitionError {
    pub from: CommandStatus,
    pub to: CommandStatus,
}

impl fmt::Display for CommandStatusTransitionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "invalid command status transition: {:?} -> {:?}",
            self.from, self.to
        )
    }
}

impl std::error::Error for CommandStatusTransitionError {}

/// SHA-256 fingerprint of canonical command operation and request bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct RequestFingerprint([u8; 32]);

impl RequestFingerprint {
    pub const fn from_bytes(value: [u8; 32]) -> Self {
        Self(value)
    }

    pub const fn as_bytes(self) -> [u8; 32] {
        self.0
    }
}

impl fmt::Display for RequestFingerprint {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        for byte in self.0 {
            write!(formatter, "{byte:02x}")?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use uuid::Uuid;

    use super::{
        ClientCommandId, ClientCommandIdError, CommandStatus, CommandStatusTransitionError,
        RequestFingerprint,
    };

    #[test]
    fn accepts_only_uuidv7_client_command_ids() {
        let uuid = Uuid::parse_str("019fe2d0-3bc5-7cf0-b3d2-0728c077a930")
            .expect("command UUID should be valid");

        assert_eq!(
            ClientCommandId::try_from_uuid(uuid).unwrap().as_uuid(),
            uuid
        );
        assert_eq!(
            ClientCommandId::try_from_uuid(
                Uuid::parse_str("019fe2d0-3bc5-4cf0-b3d2-0728c077a930")
                    .expect("non-v7 UUID should be valid")
            )
            .unwrap_err(),
            ClientCommandIdError::NotUuidV7
        );
    }

    #[test]
    fn fingerprint_is_fixed_size_and_displays_lowercase_hex() {
        let fingerprint = RequestFingerprint::from_bytes([0xab; 32]);

        assert_eq!(fingerprint.as_bytes(), [0xab; 32]);
        assert_eq!(fingerprint.to_string(), "ab".repeat(32));
    }

    #[test]
    fn command_status_allows_retry_only_before_terminal_acceptance() {
        let status = CommandStatus::Received
            .transition_to(CommandStatus::Processing)
            .unwrap()
            .transition_to(CommandStatus::FailedRetryable)
            .unwrap()
            .transition_to(CommandStatus::Processing)
            .unwrap()
            .transition_to(CommandStatus::Accepted)
            .unwrap();

        assert!(status.is_terminal());
        assert!(!status.is_recoverable());
        assert!(!status.can_transition_to(CommandStatus::Processing));
    }

    #[test]
    fn terminal_rejection_cannot_be_retried_as_processing() {
        assert_eq!(
            CommandStatus::Rejected
                .transition_to(CommandStatus::Processing)
                .unwrap_err(),
            CommandStatusTransitionError {
                from: CommandStatus::Rejected,
                to: CommandStatus::Processing,
            }
        );
    }

    #[test]
    fn command_status_persisted_vocabulary_round_trips_without_guessing() {
        let statuses = [
            (CommandStatus::Received, "received"),
            (CommandStatus::Processing, "processing"),
            (CommandStatus::Accepted, "accepted"),
            (CommandStatus::Rejected, "rejected"),
            (CommandStatus::FailedRetryable, "failed_retryable"),
        ];

        for (status, persisted) in statuses {
            assert_eq!(status.as_persisted_str(), persisted);
            assert_eq!(
                CommandStatus::try_from_persisted_str(persisted),
                Some(status)
            );
        }
        assert_eq!(CommandStatus::try_from_persisted_str("unknown"), None);
    }
}
