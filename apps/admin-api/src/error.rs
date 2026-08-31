use axum::{extract::rejection::JsonRejection, response::IntoResponse};
use football_platform::{ApplicationError, ErrorCode, HttpApplicationError, RequestId};

#[derive(Debug)]
pub(crate) struct ApiCommandError(HttpApplicationError);

impl ApiCommandError {
    pub(crate) fn new(
        error: ApplicationError,
        request_id: RequestId,
        client_command_id: Option<&str>,
    ) -> Self {
        let mut http = HttpApplicationError::new(error).with_request_id(request_id.to_string());
        if let Some(client_command_id) = client_command_id {
            http = http.with_client_command_id(client_command_id);
        }
        Self(http)
    }

    pub(crate) fn invalid_value(
        request_id: RequestId,
        client_command_id: Option<&str>,
        field: &'static str,
        message: impl Into<String>,
    ) -> Self {
        Self::new(
            ApplicationError::new(ErrorCode::ValidationInvalidValue, message)
                .with_detail("field", field),
            request_id,
            client_command_id,
        )
    }

    pub(crate) fn invalid_json(request_id: RequestId, _rejection: JsonRejection) -> Self {
        Self::new(
            ApplicationError::new(ErrorCode::RequestInvalidJson, "the request body is invalid"),
            request_id,
            None,
        )
    }
}

impl IntoResponse for ApiCommandError {
    fn into_response(self) -> axum::response::Response {
        self.0.into_response()
    }
}
