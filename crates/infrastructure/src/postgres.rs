use sqlx::{
    Error as SqlxError,
    postgres::{PgConnectOptions, PgPool, PgPoolOptions},
};
use std::{fmt, str::FromStr, time::Duration};

const DEFAULT_MIN_CONNECTIONS: u32 = 1;
const DEFAULT_MAX_CONNECTIONS: u32 = 10;
const DEFAULT_ACQUIRE_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DatabasePoolConfig {
    pub min_connections: u32,
    pub max_connections: u32,
    pub acquire_timeout: Duration,
}

impl Default for DatabasePoolConfig {
    fn default() -> Self {
        Self {
            min_connections: DEFAULT_MIN_CONNECTIONS,
            max_connections: DEFAULT_MAX_CONNECTIONS,
            acquire_timeout: DEFAULT_ACQUIRE_TIMEOUT,
        }
    }
}

impl DatabasePoolConfig {
    pub fn validate(self) -> Result<(), PoolConfigError> {
        if self.max_connections == 0 {
            return Err(PoolConfigError::MaxConnectionsMustBePositive);
        }

        if self.min_connections > self.max_connections {
            return Err(PoolConfigError::MinConnectionsExceedMax);
        }

        if self.acquire_timeout.is_zero() {
            return Err(PoolConfigError::AcquireTimeoutMustBePositive);
        }

        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PoolConfigError {
    MaxConnectionsMustBePositive,
    MinConnectionsExceedMax,
    AcquireTimeoutMustBePositive,
}

impl fmt::Display for PoolConfigError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = match self {
            Self::MaxConnectionsMustBePositive => "max_connections must be positive",
            Self::MinConnectionsExceedMax => "min_connections cannot exceed max_connections",
            Self::AcquireTimeoutMustBePositive => "acquire_timeout must be positive",
        };
        formatter.write_str(message)
    }
}

impl std::error::Error for PoolConfigError {}

#[derive(Debug)]
pub enum DatabasePoolError {
    InvalidConfig(PoolConfigError),
    InvalidUrl(SqlxError),
    Connect(SqlxError),
}

impl fmt::Display for DatabasePoolError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidConfig(error) => {
                write!(formatter, "invalid database pool config: {error}")
            }
            Self::InvalidUrl(error) => write!(formatter, "invalid database URL: {error}"),
            Self::Connect(error) => write!(formatter, "database connection failed: {error}"),
        }
    }
}

impl std::error::Error for DatabasePoolError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::InvalidConfig(error) => Some(error),
            Self::InvalidUrl(error) | Self::Connect(error) => Some(error),
        }
    }
}

fn pool_options(config: DatabasePoolConfig) -> Result<PgPoolOptions, DatabasePoolError> {
    config
        .validate()
        .map_err(DatabasePoolError::InvalidConfig)?;

    Ok(PgPoolOptions::new()
        .min_connections(config.min_connections)
        .max_connections(config.max_connections)
        .acquire_timeout(config.acquire_timeout)
        .after_connect(|connection, _metadata| {
            Box::pin(async move {
                sqlx::query("SET TIME ZONE 'UTC'")
                    .execute(connection)
                    .await?;
                Ok(())
            })
        }))
}

fn connect_options(database_url: &str) -> Result<PgConnectOptions, DatabasePoolError> {
    PgConnectOptions::from_str(database_url).map_err(DatabasePoolError::InvalidUrl)
}

pub async fn connect_database(
    database_url: &str,
    config: DatabasePoolConfig,
) -> Result<PgPool, DatabasePoolError> {
    let pool_options = pool_options(config)?;
    let options = connect_options(database_url)?;
    pool_options
        .connect_with(options)
        .await
        .map_err(DatabasePoolError::Connect)
}

/// Build a validated pool without opening a database connection yet.
///
/// HTTP adapters use this when liveness must remain available while the
/// database is unavailable. The first database operation still reports the
/// connection failure to the readiness boundary.
pub fn connect_database_lazy(
    database_url: &str,
    config: DatabasePoolConfig,
) -> Result<PgPool, DatabasePoolError> {
    let pool_options = pool_options(config)?;
    let options = connect_options(database_url)?;
    Ok(pool_options.connect_lazy_with(options))
}

pub async fn check_database_readiness(pool: &PgPool) -> Result<(), SqlxError> {
    sqlx::query("SELECT 1").execute(pool).await.map(|_| ())
}

#[cfg(test)]
mod tests {
    use super::{DatabasePoolConfig, PoolConfigError};
    use std::time::Duration;

    #[test]
    fn default_pool_config_is_valid() {
        assert!(DatabasePoolConfig::default().validate().is_ok());
    }

    #[test]
    fn rejects_zero_max_connections() {
        let config = DatabasePoolConfig {
            max_connections: 0,
            ..DatabasePoolConfig::default()
        };

        assert_eq!(
            config.validate(),
            Err(PoolConfigError::MaxConnectionsMustBePositive)
        );
    }

    #[test]
    fn rejects_min_connections_above_maximum() {
        let config = DatabasePoolConfig {
            min_connections: 11,
            max_connections: 10,
            ..DatabasePoolConfig::default()
        };

        assert_eq!(
            config.validate(),
            Err(PoolConfigError::MinConnectionsExceedMax)
        );
    }

    #[test]
    fn rejects_zero_acquire_timeout() {
        let config = DatabasePoolConfig {
            acquire_timeout: Duration::ZERO,
            ..DatabasePoolConfig::default()
        };

        assert_eq!(
            config.validate(),
            Err(PoolConfigError::AcquireTimeoutMustBePositive)
        );
    }

    #[tokio::test]
    #[ignore = "requires an explicit TEST_DATABASE_URL"]
    async fn connects_and_sets_utc_session_timezone() {
        let database_url = std::env::var("TEST_DATABASE_URL")
            .expect("TEST_DATABASE_URL must be provided for the integration probe");
        let pool = super::super::connect_database(&database_url, DatabasePoolConfig::default())
            .await
            .expect("database pool should connect");
        let timezone: String = sqlx::query_scalar("SHOW TIME ZONE")
            .fetch_one(&pool)
            .await
            .expect("session timezone should be queryable");

        assert_eq!(timezone, "UTC");
        pool.close().await;
    }
}
