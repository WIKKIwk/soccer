use std::fmt;

use axum::{extract::Request, http::HeaderValue, middleware::Next, response::Response};
use tracing::Instrument;
use uuid::Uuid;

pub const REQUEST_ID_HEADER: &str = "x-request-id";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RequestId(Uuid);

impl RequestId {
    pub fn new() -> Self {
        Self(Uuid::now_v7())
    }

    pub fn as_uuid(self) -> Uuid {
        self.0
    }
}

impl Default for RequestId {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for RequestId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

pub async fn request_id_middleware(mut request: Request, next: Next) -> Response {
    let request_id = RequestId::new();
    request.extensions_mut().insert(request_id);

    let method = request.method().clone();
    let path = request.uri().path().to_owned();
    let span = tracing::info_span!(
        "http.request",
        request_id = %request_id,
        method = %method,
        path = %path,
    );
    let mut response = next.run(request).instrument(span).await;

    tracing::info!(
        request_id = %request_id,
        method = %method,
        path = %path,
        status = response.status().as_u16(),
        "http request completed"
    );

    response.headers_mut().insert(
        REQUEST_ID_HEADER,
        HeaderValue::from_str(&request_id.to_string())
            .expect("a UUIDv7 request ID is always a valid HTTP header value"),
    );
    response
}

#[cfg(test)]
mod tests {
    use axum::{
        Router,
        body::Body,
        http::{Request, StatusCode},
        middleware,
        response::IntoResponse,
        routing::get,
    };
    use tower::ServiceExt;
    use uuid::{Uuid, Version};

    use super::{REQUEST_ID_HEADER, RequestId, request_id_middleware};

    async fn ok_handler(request: Request<Body>) -> impl IntoResponse {
        let request_id = request
            .extensions()
            .get::<RequestId>()
            .expect("request ID middleware should add the extension");
        (StatusCode::OK, request_id.to_string())
    }

    #[tokio::test]
    async fn generates_uuidv7_replaces_inbound_header_and_exposes_extension() {
        let app = Router::new()
            .route("/probe", get(ok_handler))
            .layer(middleware::from_fn(request_id_middleware));
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/probe")
                    .header(REQUEST_ID_HEADER, "client-provided-id")
                    .body(Body::empty())
                    .expect("test request should build"),
            )
            .await
            .expect("test request should succeed");

        assert_eq!(response.status(), StatusCode::OK);
        let response_header = response
            .headers()
            .get(REQUEST_ID_HEADER)
            .expect("response should carry a request ID")
            .to_str()
            .expect("request ID header should be ASCII")
            .to_owned();
        let response_id = Uuid::parse_str(&response_header).expect("request ID should be UUID");
        assert_eq!(response_id.get_version(), Some(Version::SortRand));
        assert_ne!(response_header, "client-provided-id");

        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("response body should be readable");
        assert_eq!(body, response_header.as_bytes());
    }

    #[test]
    fn request_id_display_is_canonical_uuid() {
        let request_id = RequestId::new();
        assert_eq!(request_id.to_string(), request_id.as_uuid().to_string());
    }
}
