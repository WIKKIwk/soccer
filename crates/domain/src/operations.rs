use crate::{AdminUserId, FixtureId, MatchId, UtcInstant};
use std::fmt;
use time::Duration;
use uuid::Uuid;

/// The expiry rules shared by operator-lease adapters.
///
/// A caller must provide an explicit positive whole-second duration. The
/// policy deliberately has no silent default or pilot-specific maximum until
/// real match-operation evidence establishes those values.
pub struct OperatorLeaseExpiryPolicy;

impl OperatorLeaseExpiryPolicy {
    pub const fn validate_duration_seconds(
        duration_seconds: u32,
    ) -> Result<(), OperatorLeaseExpiryPolicyError> {
        if duration_seconds == 0 {
            return Err(OperatorLeaseExpiryPolicyError::DurationMustBePositive);
        }

        Ok(())
    }

    pub fn expiry_at(
        at: UtcInstant,
        duration_seconds: u32,
    ) -> Result<UtcInstant, OperatorLeaseExpiryPolicyError> {
        Self::validate_duration_seconds(duration_seconds)?;

        at.as_offset_datetime()
            .checked_add(Duration::seconds(i64::from(duration_seconds)))
            .map(UtcInstant::from_offset_datetime)
            .ok_or(OperatorLeaseExpiryPolicyError::ExpiryOutOfRange)
    }

    pub fn is_active(expires_at: UtcInstant, at: UtcInstant) -> bool {
        expires_at > at
    }

    pub fn is_expired(expires_at: UtcInstant, at: UtcInstant) -> bool {
        expires_at <= at
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OperatorLeaseExpiryPolicyError {
    DurationMustBePositive,
    ExpiryOutOfRange,
}

impl fmt::Display for OperatorLeaseExpiryPolicyError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::DurationMustBePositive => {
                formatter.write_str("operator lease duration must be positive")
            }
            Self::ExpiryOutOfRange => {
                formatter.write_str("operator lease expiry is outside the supported time range")
            }
        }
    }
}

impl std::error::Error for OperatorLeaseExpiryPolicyError {}

/// Canonical identity of one operator lease.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct OperatorLeaseId(Uuid);

impl OperatorLeaseId {
    pub const fn from_uuid(value: Uuid) -> Self {
        Self(value)
    }

    pub const fn as_uuid(self) -> Uuid {
        self.0
    }
}

impl From<Uuid> for OperatorLeaseId {
    fn from(value: Uuid) -> Self {
        Self::from_uuid(value)
    }
}

impl From<OperatorLeaseId> for Uuid {
    fn from(value: OperatorLeaseId) -> Self {
        value.as_uuid()
    }
}

impl fmt::Display for OperatorLeaseId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

/// The minimal match-to-operator assignment needed before a claim.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MatchAssignment {
    match_id: MatchId,
    fixture_id: FixtureId,
    operator_id: AdminUserId,
}

impl MatchAssignment {
    pub const fn new(match_id: MatchId, fixture_id: FixtureId, operator_id: AdminUserId) -> Self {
        Self {
            match_id,
            fixture_id,
            operator_id,
        }
    }

    pub const fn match_id(self) -> MatchId {
        self.match_id
    }

    pub const fn fixture_id(self) -> FixtureId {
        self.fixture_id
    }

    pub const fn operator_id(self) -> AdminUserId {
        self.operator_id
    }
}

/// A lease held by the assigned operator while working on a match.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OperatorLease {
    id: OperatorLeaseId,
    match_id: MatchId,
    operator_id: AdminUserId,
    claimed_at: UtcInstant,
    expires_at: UtcInstant,
}

impl OperatorLease {
    pub fn try_new(
        id: OperatorLeaseId,
        match_id: MatchId,
        operator_id: AdminUserId,
        claimed_at: UtcInstant,
        expires_at: UtcInstant,
    ) -> Result<Self, OperatorLeaseError> {
        if expires_at <= claimed_at {
            return Err(OperatorLeaseError::ExpiryNotAfterClaim);
        }

        Ok(Self {
            id,
            match_id,
            operator_id,
            claimed_at,
            expires_at,
        })
    }

    /// Renews an active lease without changing its identity or claim time.
    pub fn try_heartbeat(
        self,
        heartbeat_at: UtcInstant,
        expires_at: UtcInstant,
    ) -> Result<Self, OperatorLeaseError> {
        if heartbeat_at < self.claimed_at {
            return Err(OperatorLeaseError::HeartbeatBeforeClaim);
        }
        if self.is_expired_at(heartbeat_at) {
            return Err(OperatorLeaseError::HeartbeatAfterExpiry);
        }
        if expires_at <= heartbeat_at {
            return Err(OperatorLeaseError::ExpiryNotAfterHeartbeat);
        }
        if expires_at <= self.expires_at {
            return Err(OperatorLeaseError::ExpiryNotExtended);
        }

        Ok(Self { expires_at, ..self })
    }

    pub fn is_active_at(self, at: UtcInstant) -> bool {
        OperatorLeaseExpiryPolicy::is_active(self.expires_at, at)
    }

    pub fn is_expired_at(self, at: UtcInstant) -> bool {
        OperatorLeaseExpiryPolicy::is_expired(self.expires_at, at)
    }

    pub const fn id(self) -> OperatorLeaseId {
        self.id
    }

    pub const fn match_id(self) -> MatchId {
        self.match_id
    }

    pub const fn operator_id(self) -> AdminUserId {
        self.operator_id
    }

    pub const fn claimed_at(self) -> UtcInstant {
        self.claimed_at
    }

    pub const fn expires_at(self) -> UtcInstant {
        self.expires_at
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OperatorLeaseError {
    ExpiryNotAfterClaim,
    HeartbeatBeforeClaim,
    HeartbeatAfterExpiry,
    ExpiryNotAfterHeartbeat,
    ExpiryNotExtended,
}

impl fmt::Display for OperatorLeaseError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ExpiryNotAfterClaim => {
                formatter.write_str("operator lease expiry must be after claim time")
            }
            Self::HeartbeatBeforeClaim => {
                formatter.write_str("operator lease heartbeat cannot precede claim time")
            }
            Self::HeartbeatAfterExpiry => {
                formatter.write_str("operator lease heartbeat cannot renew an expired lease")
            }
            Self::ExpiryNotAfterHeartbeat => {
                formatter.write_str("operator lease expiry must be after heartbeat time")
            }
            Self::ExpiryNotExtended => {
                formatter.write_str("operator lease heartbeat must extend the current expiry")
            }
        }
    }
}

impl std::error::Error for OperatorLeaseError {}

#[cfg(test)]
mod tests {
    use super::{
        MatchAssignment, OperatorLease, OperatorLeaseError, OperatorLeaseExpiryPolicy,
        OperatorLeaseExpiryPolicyError, OperatorLeaseId,
    };
    use crate::{AdminUserId, FixtureId, MatchId, UtcInstant};
    use uuid::Uuid;

    fn uuid(value: &str) -> Uuid {
        Uuid::parse_str(value).expect("operation fixture UUID should be valid")
    }

    fn match_id() -> MatchId {
        MatchId::from_uuid(uuid("019fe290-3bc5-7cf0-b3d2-0728c077a930"))
    }

    fn fixture_id() -> FixtureId {
        FixtureId::from_uuid(uuid("019fe290-3bc5-7cf1-b3d2-0728c077a930"))
    }

    fn operator_id() -> AdminUserId {
        AdminUserId::from_uuid(uuid("019fe290-3bc5-7cf2-b3d2-0728c077a930"))
    }

    fn lease_id() -> OperatorLeaseId {
        OperatorLeaseId::from_uuid(uuid("019fe290-3bc5-7cf3-b3d2-0728c077a930"))
    }

    #[test]
    fn assignment_preserves_match_fixture_and_operator_identity() {
        let assignment = MatchAssignment::new(match_id(), fixture_id(), operator_id());

        assert_eq!(assignment.match_id(), match_id());
        assert_eq!(assignment.fixture_id(), fixture_id());
        assert_eq!(assignment.operator_id(), operator_id());
    }

    #[test]
    fn lease_requires_expiry_after_claim_time() {
        let claimed_at = UtcInstant::from_unix_timestamp(1_777_000_000)
            .expect("claim timestamp should be representable");
        let expires_at = UtcInstant::from_unix_timestamp(1_777_000_060)
            .expect("expiry timestamp should be representable");
        let lease = OperatorLease::try_new(
            lease_id(),
            match_id(),
            operator_id(),
            claimed_at,
            expires_at,
        )
        .expect("future expiry should produce a lease");

        assert_eq!(lease.expires_at(), expires_at);
        assert_eq!(
            OperatorLease::try_new(
                lease_id(),
                match_id(),
                operator_id(),
                claimed_at,
                claimed_at,
            ),
            Err(OperatorLeaseError::ExpiryNotAfterClaim)
        );
    }

    #[test]
    fn expiry_policy_requires_an_explicit_positive_duration() {
        assert_eq!(
            OperatorLeaseExpiryPolicy::validate_duration_seconds(0),
            Err(OperatorLeaseExpiryPolicyError::DurationMustBePositive)
        );
        assert_eq!(
            OperatorLeaseExpiryPolicy::validate_duration_seconds(1),
            Ok(())
        );
    }

    #[test]
    fn expiry_policy_uses_utc_and_treats_the_exact_boundary_as_expired() {
        let claimed_at = UtcInstant::from_unix_timestamp(1_777_000_000)
            .expect("claim timestamp should be representable");
        let expires_at = OperatorLeaseExpiryPolicy::expiry_at(claimed_at, 60)
            .expect("positive duration should produce an expiry");

        assert_eq!(expires_at.unix_timestamp(), 1_777_000_060);
        assert_eq!(
            expires_at.as_offset_datetime().offset(),
            time::UtcOffset::UTC
        );
        assert!(OperatorLeaseExpiryPolicy::is_active(expires_at, claimed_at));
        assert!(!OperatorLeaseExpiryPolicy::is_active(
            expires_at, expires_at
        ));
        assert!(OperatorLeaseExpiryPolicy::is_expired(
            expires_at, expires_at
        ));
    }

    #[test]
    fn expiry_policy_rejects_a_time_range_overflow() {
        let at = UtcInstant::from_offset_datetime(
            time::Date::MAX.with_time(time::Time::MAX).assume_utc(),
        );

        assert_eq!(
            OperatorLeaseExpiryPolicy::expiry_at(at, 1),
            Err(OperatorLeaseExpiryPolicyError::ExpiryOutOfRange)
        );
    }

    #[test]
    fn heartbeat_extends_an_active_lease_without_changing_identity_or_claim_time() {
        let claimed_at = UtcInstant::from_unix_timestamp(1_777_000_000)
            .expect("claim timestamp should be representable");
        let original_expiry = UtcInstant::from_unix_timestamp(1_777_000_060)
            .expect("original expiry should be representable");
        let heartbeat_at = UtcInstant::from_unix_timestamp(1_777_000_030)
            .expect("heartbeat timestamp should be representable");
        let renewed_expiry = UtcInstant::from_unix_timestamp(1_777_000_090)
            .expect("renewed expiry should be representable");
        let lease = OperatorLease::try_new(
            lease_id(),
            match_id(),
            operator_id(),
            claimed_at,
            original_expiry,
        )
        .expect("initial lease should be valid");

        let renewed = lease
            .try_heartbeat(heartbeat_at, renewed_expiry)
            .expect("active lease should be renewable");

        assert_eq!(renewed.id(), lease.id());
        assert_eq!(renewed.match_id(), lease.match_id());
        assert_eq!(renewed.operator_id(), lease.operator_id());
        assert_eq!(renewed.claimed_at(), lease.claimed_at());
        assert_eq!(renewed.expires_at(), renewed_expiry);
    }

    #[test]
    fn heartbeat_rejects_backwards_expired_and_non_extending_requests() {
        let claimed_at = UtcInstant::from_unix_timestamp(1_777_000_000)
            .expect("claim timestamp should be representable");
        let original_expiry = UtcInstant::from_unix_timestamp(1_777_000_060)
            .expect("original expiry should be representable");
        let lease = OperatorLease::try_new(
            lease_id(),
            match_id(),
            operator_id(),
            claimed_at,
            original_expiry,
        )
        .expect("initial lease should be valid");

        assert_eq!(
            lease.try_heartbeat(
                UtcInstant::from_unix_timestamp(1_776_999_999).unwrap(),
                UtcInstant::from_unix_timestamp(1_777_000_090).unwrap(),
            ),
            Err(OperatorLeaseError::HeartbeatBeforeClaim)
        );
        assert_eq!(
            lease.try_heartbeat(
                original_expiry,
                UtcInstant::from_unix_timestamp(1_777_000_090).unwrap(),
            ),
            Err(OperatorLeaseError::HeartbeatAfterExpiry)
        );
        assert_eq!(
            lease.try_heartbeat(
                UtcInstant::from_unix_timestamp(1_777_000_030).unwrap(),
                UtcInstant::from_unix_timestamp(1_777_000_030).unwrap(),
            ),
            Err(OperatorLeaseError::ExpiryNotAfterHeartbeat)
        );
        assert_eq!(
            lease.try_heartbeat(
                UtcInstant::from_unix_timestamp(1_777_000_030).unwrap(),
                original_expiry,
            ),
            Err(OperatorLeaseError::ExpiryNotExtended)
        );
    }
}
