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
        Err(_) => (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(HealthResponse {
                status: "not_ready",
            }),
        ),
    }
}
