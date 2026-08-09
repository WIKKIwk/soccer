use football_domain::AdminUserId;
use std::fmt;

/// Explicit input for creating the controlled development administrator.
///
/// This is deliberately an application input rather than a process-startup
/// default. The caller must provide the identity and choose when persistence is
/// performed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DevelopmentAdministrator {
    id: AdminUserId,
    username: String,
    display_name: String,
}

impl DevelopmentAdministrator {
    pub fn new(
        id: AdminUserId,
        username: impl Into<String>,
        display_name: impl Into<String>,
    ) -> Result<Self, DevelopmentAdministratorError> {
        let username = username.into();
        if username.trim().is_empty() {
            return Err(DevelopmentAdministratorError::EmptyUsername);
        }

        let display_name = display_name.into();
        if display_name.trim().is_empty() {
            return Err(DevelopmentAdministratorError::EmptyDisplayName);
        }

        Ok(Self {
            id,
            username,
            display_name,
        })
    }

    pub const fn id(&self) -> AdminUserId {
        self.id
    }

    pub fn username(&self) -> &str {
        &self.username
    }

    pub fn display_name(&self) -> &str {
        &self.display_name
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DevelopmentAdministratorError {
    EmptyUsername,
    EmptyDisplayName,
}

impl fmt::Display for DevelopmentAdministratorError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = match self {
            Self::EmptyUsername => "development administrator username must not be empty",
            Self::EmptyDisplayName => "development administrator display name must not be empty",
        };
        formatter.write_str(message)
    }
}

impl std::error::Error for DevelopmentAdministratorError {}

#[cfg(test)]
mod tests {
    use football_domain::AdminUserId;
    use uuid::Uuid;

    use super::{DevelopmentAdministrator, DevelopmentAdministratorError};

    fn admin_id() -> AdminUserId {
        AdminUserId::from_uuid(
            Uuid::parse_str("019fe250-3bc5-7cf0-b3d2-0728c077a930")
                .expect("fixture UUID should be valid"),
        )
    }

    #[test]
    fn accepts_explicit_development_identity_without_mutating_text() {
        let admin = DevelopmentAdministrator::new(admin_id(), "dev-admin", "Development Admin")
            .expect("valid development identity should be accepted");

        assert_eq!(admin.id(), admin_id());
        assert_eq!(admin.username(), "dev-admin");
        assert_eq!(admin.display_name(), "Development Admin");
    }

    #[test]
    fn rejects_blank_identity_values() {
        assert_eq!(
            DevelopmentAdministrator::new(admin_id(), " ", "Development Admin"),
            Err(DevelopmentAdministratorError::EmptyUsername)
        );
        assert_eq!(
            DevelopmentAdministrator::new(admin_id(), "dev-admin", "\t"),
            Err(DevelopmentAdministratorError::EmptyDisplayName)
        );
    }
}
