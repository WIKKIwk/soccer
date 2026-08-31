#![forbid(unsafe_code)]

mod auth;
mod command_input;
mod command_response;
mod error;
mod health;
mod lifecycle;
mod live_actions;

use axum::{Json, Router, middleware, routing::get};
use football_contracts::{ADMIN_API_MAJOR_VERSION, ADMIN_API_PREFIX};
use football_infrastructure::DatabasePool;
use football_platform::request_id_middleware;
use serde::Serialize;

#[derive(Debug, Serialize)]
struct ApiInfo {
    service: &'static str,
    api_version: u16,
}

async fn api_info() -> Json<ApiInfo> {
    Json(ApiInfo {
        service: "football-admin-api",
        api_version: ADMIN_API_MAJOR_VERSION,
    })
}

pub fn build_router(database: DatabasePool) -> Router {
    Router::new()
        .route(health::LIVENESS_PATH, get(health::liveness))
        .route(health::READINESS_PATH, get(health::readiness))
        .route(ADMIN_API_PREFIX, get(api_info))
        .merge(lifecycle::router())
        .merge(live_actions::router())
        .with_state(database)
        .layer(middleware::from_fn(request_id_middleware))
}

#[cfg(test)]
mod tests {
    use axum::{
        body::Body,
        http::{Request, StatusCode},
    };
    use football_infrastructure::{DatabasePoolConfig, connect_database_lazy};
    use serde_json::Value;
    use std::time::Duration;
    use tower::ServiceExt;

    use super::build_router;

    fn unavailable_pool() -> football_infrastructure::DatabasePool {
        connect_database_lazy(
            "postgres://admin-api-test@127.0.0.1:1/football",
            DatabasePoolConfig {
                min_connections: 0,
                max_connections: 1,
                acquire_timeout: Duration::from_millis(50),
            },
        )
        .unwrap()
    }

    #[tokio::test]
    async fn command_routes_fail_closed_before_database_access() {
        let response = build_router(unavailable_pool())
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/admin/v1/matches/not-a-uuid/goals")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        r#"{
                            "command": {
                                "command_id":"019ff700-3bc5-7cf1-b3d2-0728c077a930",
                                "client_command_id":"019ff700-3bc5-7cf2-b3d2-0728c077a930",
                                "event_id":"019ff700-3bc5-7cf3-b3d2-0728c077a930",
                                "operator_lease_id":"019ff700-3bc5-7cf4-b3d2-0728c077a930",
                                "expected_version":1,
                                "clock_ms":1000,
                                "occurred_at":"2026-08-31T10:00:00Z"
                            },
                            "period":"first_half",
                            "team_id":"019ff700-3bc5-7cf5-b3d2-0728c077a930",
                            "award_type":"regular"
                        }"#,
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let body: Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(body["error"]["code"], "authentication.required");
    }

    #[tokio::test]
    async fn malformed_command_json_uses_the_stable_error_contract() {
        let response = build_router(unavailable_pool())
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/admin/v1/matches/019ff700-3bc5-7cf0-b3d2-0728c077a930/finish")
                    .header("content-type", "application/json")
                    .body(Body::from("{"))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let body: Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(body["error"]["code"], "request.invalid_json");
    }
}
