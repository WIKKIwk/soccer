use std::{env, fmt, net::SocketAddr};

pub const DATABASE_URL_ENV: &str = "DATABASE_URL";
pub const ADMIN_API_BIND_ADDR_ENV: &str = "ADMIN_API_BIND_ADDR";
pub const DEFAULT_ADMIN_API_BIND_ADDR: &str = "127.0.0.1:3000";
pub const PUBLIC_API_BIND_ADDR_ENV: &str = "PUBLIC_API_BIND_ADDR";
pub const DEFAULT_PUBLIC_API_BIND_ADDR: &str = "127.0.0.1:3001";
pub const LOG_FILTER_ENV: &str = "RUST_LOG";
pub const DEFAULT_LOG_FILTER: &str = "info";

/// Typed process configuration shared by application binaries.
///
/// The database URL remains private so callers use the accessor and do not
/// spread environment-variable reads through handlers or services.
#[derive(Clone, PartialEq, Eq)]
pub struct ApplicationConfig {
    database_url: String,
    admin_api_bind_addr: SocketAddr,
    public_api_bind_addr: SocketAddr,
    log_filter: String,
}

impl fmt::Debug for ApplicationConfig {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ApplicationConfig")
            .field("database_url", &"<redacted>")
            .field("admin_api_bind_addr", &self.admin_api_bind_addr)
            .field("public_api_bind_addr", &self.public_api_bind_addr)
            .field("log_filter", &self.log_filter)
            .finish()
    }
}

impl ApplicationConfig {
    /// Load process configuration from the environment.
    pub fn from_env() -> Result<Self, ConfigError> {
        let database_url = read_environment(DATABASE_URL_ENV)?;
        let admin_api_bind_addr = read_environment(ADMIN_API_BIND_ADDR_ENV)?;
        let public_api_bind_addr = read_environment(PUBLIC_API_BIND_ADDR_ENV)?;
        let log_filter = read_environment(LOG_FILTER_ENV)?;
        Self::from_raw_values(
            database_url,
            admin_api_bind_addr,
            public_api_bind_addr,
            log_filter,
        )
    }

    pub fn database_url(&self) -> &str {
        &self.database_url
    }

    pub const fn admin_api_bind_addr(&self) -> SocketAddr {
        self.admin_api_bind_addr
    }

    pub const fn public_api_bind_addr(&self) -> SocketAddr {
        self.public_api_bind_addr
    }

    pub fn log_filter(&self) -> &str {
        &self.log_filter
    }

    fn from_raw_values(
        database_url: Option<String>,
        admin_api_bind_addr: Option<String>,
        public_api_bind_addr: Option<String>,
        log_filter: Option<String>,
    ) -> Result<Self, ConfigError> {
        let database_url = required_value(DATABASE_URL_ENV, database_url)?;
        let admin_api_bind_addr = match admin_api_bind_addr {
            Some(value) => value
                .parse()
                .map_err(|_| ConfigError::InvalidSocketAddress {
                    name: ADMIN_API_BIND_ADDR_ENV,
                })?,
            None => DEFAULT_ADMIN_API_BIND_ADDR
                .parse()
                .expect("default admin API bind address must be valid"),
        };
        let public_api_bind_addr = match public_api_bind_addr {
            Some(value) => value
                .parse()
                .map_err(|_| ConfigError::InvalidSocketAddress {
                    name: PUBLIC_API_BIND_ADDR_ENV,
                })?,
            None => DEFAULT_PUBLIC_API_BIND_ADDR
                .parse()
                .expect("default public API bind address must be valid"),
        };
        let log_filter = log_filter.unwrap_or_else(|| DEFAULT_LOG_FILTER.to_owned());

        Ok(Self {
            database_url,
            admin_api_bind_addr,
            public_api_bind_addr,
            log_filter,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfigError {
    MissingRequired { name: &'static str },
    EmptyRequired { name: &'static str },
    NonUnicode { name: &'static str },
    InvalidSocketAddress { name: &'static str },
}

impl fmt::Display for ConfigError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingRequired { name } => {
                write!(formatter, "required environment variable {name} is missing")
            }
            Self::EmptyRequired { name } => {
                write!(formatter, "required environment variable {name} is empty")
            }
            Self::NonUnicode { name } => {
                write!(
                    formatter,
                    "environment variable {name} is not valid Unicode"
                )
            }
            Self::InvalidSocketAddress { name } => {
                write!(
                    formatter,
                    "environment variable {name} is not a valid socket address"
                )
            }
        }
    }
}

impl std::error::Error for ConfigError {}

fn read_environment(name: &'static str) -> Result<Option<String>, ConfigError> {
    match env::var(name) {
        Ok(value) => Ok(Some(value)),
        Err(env::VarError::NotPresent) => Ok(None),
        Err(env::VarError::NotUnicode(_)) => Err(ConfigError::NonUnicode { name }),
    }
}

fn required_value(name: &'static str, value: Option<String>) -> Result<String, ConfigError> {
    let value = value.ok_or(ConfigError::MissingRequired { name })?;
    if value.trim().is_empty() {
        return Err(ConfigError::EmptyRequired { name });
    }

    Ok(value)
}

#[cfg(test)]
mod tests {
    use super::{
        ADMIN_API_BIND_ADDR_ENV, ApplicationConfig, ConfigError, DATABASE_URL_ENV,
        DEFAULT_ADMIN_API_BIND_ADDR, DEFAULT_LOG_FILTER, DEFAULT_PUBLIC_API_BIND_ADDR,
        PUBLIC_API_BIND_ADDR_ENV,
    };

    #[test]
    fn builds_typed_config_and_uses_default_bind_address() {
        let config = ApplicationConfig::from_raw_values(
            Some("postgres://app@127.0.0.1/football".to_owned()),
            None,
            None,
            None,
        )
        .expect("valid raw values should build configuration");

        assert_eq!(config.database_url(), "postgres://app@127.0.0.1/football");
        assert_eq!(
            config.admin_api_bind_addr().to_string(),
            DEFAULT_ADMIN_API_BIND_ADDR
        );
        assert_eq!(
            config.public_api_bind_addr().to_string(),
            DEFAULT_PUBLIC_API_BIND_ADDR
        );
        assert_eq!(config.log_filter(), DEFAULT_LOG_FILTER);
    }

    #[test]
    fn rejects_missing_database_url() {
        assert!(matches!(
            ApplicationConfig::from_raw_values(None, None, None, None),
            Err(ConfigError::MissingRequired { name }) if name == DATABASE_URL_ENV
        ));
    }

    #[test]
    fn rejects_empty_database_url() {
        assert!(matches!(
            ApplicationConfig::from_raw_values(Some("  ".to_owned()), None, None, None),
            Err(ConfigError::EmptyRequired { name }) if name == DATABASE_URL_ENV
        ));
    }

    #[test]
    fn rejects_invalid_admin_bind_address() {
        assert!(matches!(
            ApplicationConfig::from_raw_values(
                Some("postgres://app@127.0.0.1/football".to_owned()),
                Some("not-a-socket".to_owned()),
                None,
                None,
            ),
            Err(ConfigError::InvalidSocketAddress { name })
                if name == ADMIN_API_BIND_ADDR_ENV
        ));
    }

    #[test]
    fn rejects_invalid_public_bind_address() {
        assert!(matches!(
            ApplicationConfig::from_raw_values(
                Some("postgres://app@127.0.0.1/football".to_owned()),
                None,
                Some("not-a-socket".to_owned()),
                None,
            ),
            Err(ConfigError::InvalidSocketAddress { name })
                if name == PUBLIC_API_BIND_ADDR_ENV
        ));
    }

    #[test]
    fn debug_output_redacts_database_url() {
        let database_url = "postgres://app:super-secret@127.0.0.1/football";
        let config = ApplicationConfig::from_raw_values(
            Some(database_url.to_owned()),
            Some("127.0.0.1:3100".to_owned()),
            Some("127.0.0.1:3101".to_owned()),
            Some("football_admin_api=debug".to_owned()),
        )
        .expect("valid raw values should build configuration");

        let debug_output = format!("{config:?}");

        assert!(!debug_output.contains(database_url));
        assert!(!debug_output.contains("super-secret"));
        assert!(debug_output.contains("<redacted>"));
        assert!(debug_output.contains("127.0.0.1:3100"));
        assert!(debug_output.contains("127.0.0.1:3101"));
        assert!(debug_output.contains("football_admin_api=debug"));
    }
}
