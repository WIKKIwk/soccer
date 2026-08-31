use std::{env, error::Error, fmt};

pub(crate) const AUTHORIZATION_VALUE: &str = "1";

/// Explicit development-only authorization boundaries for CLI persistence.
///
/// These checks are a controlled local adapter guard, not application
/// authentication. Production authorization remains owned by the application
/// and API boundaries.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AuthorizationScope {
    DevelopmentAdmin,
    Directory,
    Competition,
    Scheduling,
    Operations,
}

impl AuthorizationScope {
    pub(crate) const fn environment_name(self) -> &'static str {
        match self {
            Self::DevelopmentAdmin => "ALLOW_DEVELOPMENT_ADMIN_SEED",
            Self::Directory => "ALLOW_DEVELOPMENT_DIRECTORY_SEED",
            Self::Competition => "ALLOW_DEVELOPMENT_COMPETITION_SEED",
            Self::Scheduling => "ALLOW_DEVELOPMENT_SCHEDULING_SEED",
            Self::Operations => "ALLOW_DEVELOPMENT_OPERATIONS_SEED",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct AuthorizationError {
    scope: AuthorizationScope,
}

impl AuthorizationError {
    pub(crate) const fn for_scope(scope: AuthorizationScope) -> Self {
        Self { scope }
    }

    pub(crate) const fn scope(self) -> AuthorizationScope {
        self.scope
    }
}

impl fmt::Display for AuthorizationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.scope {
            AuthorizationScope::DevelopmentAdmin => write!(
                formatter,
                "set {}={} to authorize development-admin persistence",
                AuthorizationScope::DevelopmentAdmin.environment_name(),
                AUTHORIZATION_VALUE
            ),
            AuthorizationScope::Directory => write!(
                formatter,
                "set {}={} to authorize directory persistence",
                AuthorizationScope::Directory.environment_name(),
                AUTHORIZATION_VALUE
            ),
            AuthorizationScope::Competition => write!(
                formatter,
                "set {}={} to authorize competition and season persistence",
                AuthorizationScope::Competition.environment_name(),
                AUTHORIZATION_VALUE
            ),
            AuthorizationScope::Scheduling => write!(
                formatter,
                "set {}={} to authorize fixture scheduling",
                AuthorizationScope::Scheduling.environment_name(),
                AUTHORIZATION_VALUE
            ),
            AuthorizationScope::Operations => write!(
                formatter,
                "set {}={} to authorize match assignment and claim persistence",
                AuthorizationScope::Operations.environment_name(),
                AUTHORIZATION_VALUE
            ),
        }
    }
}

impl Error for AuthorizationError {}

pub(crate) fn require(scope: AuthorizationScope) -> Result<(), AuthorizationError> {
    let value = env::var(scope.environment_name()).ok();
    authorize(scope, value.as_deref())
}

fn authorize(scope: AuthorizationScope, value: Option<&str>) -> Result<(), AuthorizationError> {
    if value == Some(AUTHORIZATION_VALUE) {
        return Ok(());
    }

    Err(AuthorizationError::for_scope(scope))
}

#[cfg(test)]
mod tests {
    use super::{AUTHORIZATION_VALUE, AuthorizationScope, authorize};

    const SCOPES: [AuthorizationScope; 5] = [
        AuthorizationScope::DevelopmentAdmin,
        AuthorizationScope::Directory,
        AuthorizationScope::Competition,
        AuthorizationScope::Scheduling,
        AuthorizationScope::Operations,
    ];

    #[test]
    fn every_development_boundary_requires_the_exact_confirmation_value() {
        for scope in SCOPES {
            assert!(authorize(scope, Some(AUTHORIZATION_VALUE)).is_ok());
            assert!(authorize(scope, None).is_err());
            assert!(authorize(scope, Some("0")).is_err());
            assert!(authorize(scope, Some("true")).is_err());
        }
    }

    #[test]
    fn scopes_keep_the_existing_environment_names() {
        assert_eq!(
            AuthorizationScope::DevelopmentAdmin.environment_name(),
            "ALLOW_DEVELOPMENT_ADMIN_SEED"
        );
        assert_eq!(
            AuthorizationScope::Directory.environment_name(),
            "ALLOW_DEVELOPMENT_DIRECTORY_SEED"
        );
        assert_eq!(
            AuthorizationScope::Competition.environment_name(),
            "ALLOW_DEVELOPMENT_COMPETITION_SEED"
        );
        assert_eq!(
            AuthorizationScope::Scheduling.environment_name(),
            "ALLOW_DEVELOPMENT_SCHEDULING_SEED"
        );
        assert_eq!(
            AuthorizationScope::Operations.environment_name(),
            "ALLOW_DEVELOPMENT_OPERATIONS_SEED"
        );
    }
}
