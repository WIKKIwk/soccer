#![forbid(unsafe_code)]

//! Process-level configuration, observability, HTTP wiring, and shared errors.

pub mod config;
pub mod errors;
pub mod http;
pub mod observability;

pub const DATABASE_TIMEZONE: &str = "UTC";
pub const OPERATOR_DEFAULT_TIMEZONE: &str = "Asia/Tashkent";

pub use config::{
    ADMIN_API_BIND_ADDR_ENV, ApplicationConfig, ConfigError, DATABASE_URL_ENV,
    DEFAULT_ADMIN_API_BIND_ADDR, DEFAULT_LOG_FILTER, LOG_FILTER_ENV,
};
pub use errors::{ApplicationError, ErrorCategory, ErrorCode};
pub use http::{
    HttpApplicationError, REQUEST_ID_HEADER, RequestId, map_application_error,
    request_id_middleware, status_for_category,
};
pub use observability::{LoggingInitError, init_logging};
