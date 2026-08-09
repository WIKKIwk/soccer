use std::fmt;

/// Match-level authority state.
///
/// This is deliberately separate from `EventVerificationStatus`, which
/// describes one immutable event envelope. A live operator event may remain
/// provisional even while a later match-level verification decision changes
/// the published result state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MatchVerificationStatus {
    Provisional,
    Verified,
}

impl MatchVerificationStatus {
    pub const fn initial() -> Self {
        Self::Provisional
    }

    pub const fn as_persisted_str(self) -> &'static str {
        match self {
            Self::Provisional => "provisional",
            Self::Verified => "verified",
        }
    }

    pub fn try_from_persisted_str(value: &str) -> Option<Self> {
        match value {
            "provisional" => Some(Self::Provisional),
            "verified" => Some(Self::Verified),
            _ => None,
        }
    }

    pub const fn is_provisional(self) -> bool {
        matches!(self, Self::Provisional)
    }

    pub const fn is_verified(self) -> bool {
        matches!(self, Self::Verified)
    }

    pub const fn can_transition_to(self, next: Self) -> bool {
        matches!(
            (self, next),
            (Self::Provisional, Self::Verified) | (Self::Verified, Self::Provisional)
        )
    }

    pub fn transition_to(self, next: Self) -> Result<Self, MatchVerificationTransitionError> {
        if self.can_transition_to(next) {
            Ok(next)
        } else {
            Err(MatchVerificationTransitionError {
                from: self,
                to: next,
            })
        }
    }
}

impl fmt::Display for MatchVerificationStatus {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_persisted_str())
    }
}

/// Rejection returned when a match verification state is repeated without a
/// distinct decision.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MatchVerificationTransitionError {
    pub from: MatchVerificationStatus,
    pub to: MatchVerificationStatus,
}

impl fmt::Display for MatchVerificationTransitionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "invalid match verification transition: {} -> {}",
            self.from, self.to
        )
    }
}

impl std::error::Error for MatchVerificationTransitionError {}

#[cfg(test)]
mod tests {
    use super::{MatchVerificationStatus, MatchVerificationTransitionError};

    #[test]
    fn starts_provisional_and_preserves_the_stable_vocabulary() {
        assert_eq!(
            MatchVerificationStatus::initial(),
            MatchVerificationStatus::Provisional
        );
        assert!(MatchVerificationStatus::Provisional.is_provisional());
        assert!(!MatchVerificationStatus::Provisional.is_verified());
        assert_eq!(
            MatchVerificationStatus::Provisional.as_persisted_str(),
            "provisional"
        );
        assert_eq!(
            MatchVerificationStatus::Verified.as_persisted_str(),
            "verified"
        );
        assert_eq!(
            MatchVerificationStatus::try_from_persisted_str("provisional"),
            Some(MatchVerificationStatus::Provisional)
        );
        assert_eq!(
            MatchVerificationStatus::try_from_persisted_str("verified"),
            Some(MatchVerificationStatus::Verified)
        );
        assert_eq!(
            MatchVerificationStatus::try_from_persisted_str("operator"),
            None
        );
    }

    #[test]
    fn allows_only_a_distinct_verify_or_reopen_transition() {
        assert_eq!(
            MatchVerificationStatus::Provisional.transition_to(MatchVerificationStatus::Verified),
            Ok(MatchVerificationStatus::Verified)
        );
        assert_eq!(
            MatchVerificationStatus::Verified.transition_to(MatchVerificationStatus::Provisional),
            Ok(MatchVerificationStatus::Provisional)
        );
        assert_eq!(
            MatchVerificationStatus::Provisional
                .transition_to(MatchVerificationStatus::Provisional),
            Err(MatchVerificationTransitionError {
                from: MatchVerificationStatus::Provisional,
                to: MatchVerificationStatus::Provisional,
            })
        );
        assert_eq!(
            MatchVerificationStatus::Verified.transition_to(MatchVerificationStatus::Verified),
            Err(MatchVerificationTransitionError {
                from: MatchVerificationStatus::Verified,
                to: MatchVerificationStatus::Verified,
            })
        );
    }
}
