pub mod request_id;

use axum::{
    Json,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use football_contracts::{ApiErrorBody, ApiErrorMeta, ApiErrorResponse};

use crate::{ApplicationError, ErrorCategory};

pub use request_id::{REQUEST_ID_HEADER, RequestId, request_id_middleware};

#[derive(Debug, Clone)]
pub struct HttpApplicationError {
    error: ApplicationError,
    meta: ApiErrorMeta,
}

impl HttpApplicationError {
    pub fn new(error: ApplicationError) -> Self {
        Self {
            error,
            meta: ApiErrorMeta::default(),
        }
    }

    pub fn with_request_id(mut self, request_id: impl Into<String>) -> Self {
        self.meta.request_id = Some(request_id.into());
        self
    }

    pub fn with_client_command_id(mut self, command_id: impl Into<String>) -> Self {
        self.meta.client_command_id = Some(command_id.into());
        self
    }

    pub fn error(&self) -> &ApplicationError {
        &self.error
    }

    pub fn meta(&self) -> &ApiErrorMeta {
        &self.meta
    }
}

impl From<ApplicationError> for HttpApplicationError {
    fn from(error: ApplicationError) -> Self {
        Self::new(error)
    }
}

impl IntoResponse for HttpApplicationError {
    fn into_response(self) -> Response {
        let (status, body) = map_application_error(&self.error, self.meta);
        (status, Json(body)).into_response()
    }
}

pub fn status_for_category(category: ErrorCategory) -> StatusCode {
    match category {
        ErrorCategory::Request => StatusCode::BAD_REQUEST,
        ErrorCategory::Authentication => StatusCode::UNAUTHORIZED,
        ErrorCategory::Authorization => StatusCode::FORBIDDEN,
        ErrorCategory::NotFound => StatusCode::NOT_FOUND,
        ErrorCategory::Validation | ErrorCategory::Domain => StatusCode::UNPROCESSABLE_ENTITY,
        ErrorCategory::Conflict | ErrorCategory::Idempotency => StatusCode::CONFLICT,
        ErrorCategory::RateLimit => StatusCode::TOO_MANY_REQUESTS,
        ErrorCategory::Dependency => StatusCode::SERVICE_UNAVAILABLE,
        ErrorCategory::Internal => StatusCode::INTERNAL_SERVER_ERROR,
    }
}

pub fn map_application_error(
    error: &ApplicationError,
    meta: ApiErrorMeta,
) -> (StatusCode, ApiErrorResponse) {
    let status = status_for_category(error.category());
    let body = ApiErrorResponse {
        error: ApiErrorBody {
            code: error.code().as_str().to_owned(),
            category: error.category().as_str().to_owned(),
            message: error.message().to_owned(),
            details: error.details().clone(),
            retryable: error.retryable(),
        },
        meta,
    };

    (status, body)
}

#[cfg(test)]
mod tests {
    use axum::http::StatusCode;
    use football_contracts::ApiErrorMeta;

    use crate::{ApplicationError, ErrorCategory, ErrorCode};

    use super::{map_application_error, status_for_category};

    #[test]
    fn maps_arc_categories_to_http_statuses() {
        assert_eq!(
            status_for_category(ErrorCategory::Request),
            StatusCode::BAD_REQUEST
        );
        assert_eq!(
            status_for_category(ErrorCategory::Authentication),
            StatusCode::UNAUTHORIZED
        );
        assert_eq!(
            status_for_category(ErrorCategory::Authorization),
            StatusCode::FORBIDDEN
        );
        assert_eq!(
            status_for_category(ErrorCategory::NotFound),
            StatusCode::NOT_FOUND
        );
        assert_eq!(
            status_for_category(ErrorCategory::Validation),
            StatusCode::UNPROCESSABLE_ENTITY
        );
        assert_eq!(
            status_for_category(ErrorCategory::Conflict),
            StatusCode::CONFLICT
        );
        assert_eq!(
            status_for_category(ErrorCategory::Idempotency),
            StatusCode::CONFLICT
        );
        assert_eq!(
            status_for_category(ErrorCategory::Domain),
            StatusCode::UNPROCESSABLE_ENTITY
        );
        assert_eq!(
            status_for_category(ErrorCategory::RateLimit),
            StatusCode::TOO_MANY_REQUESTS
        );
        assert_eq!(
            status_for_category(ErrorCategory::Dependency),
            StatusCode::SERVICE_UNAVAILABLE
        );
        assert_eq!(
            status_for_category(ErrorCategory::Internal),
            StatusCode::INTERNAL_SERVER_ERROR
        );
    }

    #[test]
    fn maps_application_error_to_contract_without_changing_semantics() {
        let error = ApplicationError::new(
            ErrorCode::DependencyTimeout,
            "the database operation timed out",
        )
        .with_detail("operation", "pool_acquire")
        .with_detail("attempt", "1");
        let meta = ApiErrorMeta {
            request_id: Some("request-1".to_owned()),
            client_command_id: Some("command-1".to_owned()),
        };

        let (status, response) = map_application_error(&error, meta.clone());

        assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(response.error.code, "dependency.timeout");
        assert_eq!(response.error.category, "dependency");
        assert_eq!(response.error.message, "the database operation timed out");
        assert_eq!(response.error.details["operation"], "pool_acquire");
        assert!(response.error.retryable);
        assert_eq!(response.meta, meta);

        let json = serde_json::to_value(response).expect("error contract should serialize");
        assert_eq!(json["error"]["code"], "dependency.timeout");
        assert_eq!(json["error"]["category"], "dependency");
        assert_eq!(json["error"]["details"]["attempt"], "1");
        assert_eq!(json["error"]["retryable"], true);
        assert_eq!(json["meta"]["request_id"], "request-1");
        assert_eq!(json["meta"]["client_command_id"], "command-1");
    }

    #[test]
    fn wrapper_can_carry_request_metadata() {
        let error = ApplicationError::new(ErrorCode::ValidationRequiredField, "field is required");
        let wrapper = super::HttpApplicationError::new(error)
            .with_request_id("request-2")
            .with_client_command_id("command-2");

        assert_eq!(wrapper.meta().request_id.as_deref(), Some("request-2"));
        assert_eq!(
            wrapper.meta().client_command_id.as_deref(),
            Some("command-2")
        );
    }
}
