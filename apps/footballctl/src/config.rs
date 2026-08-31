use football_infrastructure::{
    DatabasePool, DatabasePoolConfig, DatabasePoolError, connect_database as open_database,
};
use football_platform::{ApplicationConfig, ConfigError};

/// Runtime configuration owned by the footballctl process boundary.
///
/// The platform configuration remains the source of truth for environment
/// parsing and validation. This adapter adds the CLI's one shared database
/// connection path without spreading environment reads or pool defaults
/// across command handlers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CliRuntimeConfig {
    application: ApplicationConfig,
}

impl CliRuntimeConfig {
    pub(crate) fn from_env() -> Result<Self, ConfigError> {
        ApplicationConfig::from_env().map(Self::from_application)
    }

    fn from_application(application: ApplicationConfig) -> Self {
        Self { application }
    }

    pub(crate) async fn connect_database(&self) -> Result<DatabasePool, DatabasePoolError> {
        open_database(
            self.application.database_url(),
            DatabasePoolConfig::default(),
        )
        .await
    }
}
