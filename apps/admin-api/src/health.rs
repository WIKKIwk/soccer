use axum::{Json, extract::State, http::StatusCode, response::IntoResponse};
use football_infrastructure::{DatabasePool, check_database_readiness};
use serde::Serialize;

pub const LIVENESS_PATH: &str = "/healthz";
pub const READINESS_PATH: &str = "/readyz";

#[derive(Debug, Serialize, PartialEq, Eq)]
struct HealthResponse {
    status: &'static str,
}

pub async fn liveness() -> impl IntoResponse {
    (StatusCode::OK, Json(HealthResponse { status: "ok" }))
}

pub async fn readiness(State(database): State<DatabasePool>) -> impl IntoResponse {
    match check_database_readiness(&database).await {
        Ok(()) => (StatusCode::OK, Json(HealthResponse { status: "ready" })),
        Err(_) => {
            tracing::warn!(dependency = "database", "database readiness check failed");
            (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(HealthResponse {
                    status: "not_ready",
                }),
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use axum::{
        Router,
        body::Body,
        http::{Request, StatusCode},
        routing::get,
    };
    use football_infrastructure::{DatabasePoolConfig, connect_database_lazy};
    use std::time::Duration;
    use tower::ServiceExt;

    use super::{LIVENESS_PATH, READINESS_PATH, liveness, readiness};

    #[tokio::test]
    async fn liveness_is_available_without_a_database() {
        let app = Router::new().route(LIVENESS_PATH, get(liveness));
        let response = app
            .oneshot(
                Request::builder()
                    .uri(LIVENESS_PATH)
                    .body(Body::empty())
                    .expect("liveness request should build"),
            )
            .await
            .expect("liveness request should succeed");

        assert_eq!(response.status(), StatusCode::OK);
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("liveness response body should be readable");
        assert_eq!(body.as_ref(), br#"{"status":"ok"}"#);
    }

    #[tokio::test]
    async fn readiness_returns_public_503_without_dependency_details() {
        let database = connect_database_lazy(
            "postgres://health-test@127.0.0.1:1/football",
            DatabasePoolConfig {
                min_connections: 0,
                max_connections: 1,
                acquire_timeout: Duration::from_millis(100),
            },
        )
        .expect("a syntactically valid lazy database URL should build");
        let app = Router::new()
            .route(READINESS_PATH, get(readiness))
            .with_state(database);
        let response = app
            .oneshot(
                Request::builder()
                    .uri(READINESS_PATH)
                    .body(Body::empty())
                    .expect("readiness request should build"),
            )
            .await
            .expect("readiness request should return a response");

        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("readiness response body should be readable");
        assert_eq!(body.as_ref(), br#"{"status":"not_ready"}"#);
    }
}
