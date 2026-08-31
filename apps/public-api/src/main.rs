#![forbid(unsafe_code)]

mod health;
mod matches;

use axum::{Router, middleware, routing::get};
use football_infrastructure::{DatabasePool, DatabasePoolConfig, connect_database_lazy};
use football_platform::{ApplicationConfig, init_logging, request_id_middleware};
use std::error::Error;
use tokio::net::TcpListener;
use tracing::info;

fn router(database: DatabasePool) -> Router {
    Router::new()
        .route(health::LIVENESS_PATH, get(health::liveness))
        .route(health::READINESS_PATH, get(health::readiness))
        .route(matches::MATCH_PATH, get(matches::detail))
        .route(matches::MATCH_TIMELINE_PATH, get(matches::timeline))
        .with_state(database)
        .layer(middleware::from_fn(request_id_middleware))
}

#[tokio::main]
async fn main() {
    if let Err(error) = run().await {
        eprintln!("football-public-api failed: {error}");
        std::process::exit(1);
    }
}

async fn run() -> Result<(), Box<dyn Error + Send + Sync>> {
    let config = ApplicationConfig::from_env()?;
    init_logging(config.log_filter())?;
    let address = config.public_api_bind_addr();
    let database = connect_database_lazy(config.database_url(), DatabasePoolConfig::default())?;
    let listener = TcpListener::bind(address).await?;

    info!(
        service = "football-public-api",
        bind_address = %address,
        route = matches::MATCH_PATH,
        "public API listening"
    );
    axum::serve(listener, router(database)).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::router;
    use axum::{
        body::{Body, to_bytes},
        http::{Request, StatusCode},
    };
    use football_infrastructure::{DatabasePoolConfig, connect_database_lazy};
    use std::time::Duration;
    use tower::ServiceExt;

    fn unavailable_database() -> football_infrastructure::DatabasePool {
        connect_database_lazy(
            "postgres://public-test@127.0.0.1:1/football",
            DatabasePoolConfig {
                min_connections: 0,
                max_connections: 1,
                acquire_timeout: Duration::from_millis(50),
            },
        )
        .expect("lazy public test pool should build")
    }

    #[tokio::test]
    async fn liveness_does_not_require_the_database() {
        let response = router(unavailable_database())
            .oneshot(
                Request::builder()
                    .uri("/healthz")
                    .body(Body::empty())
                    .expect("health request should build"),
            )
            .await
            .expect("health request should complete");
        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn invalid_match_id_fails_before_database_access() {
        let response = router(unavailable_database())
            .oneshot(
                Request::builder()
                    .uri("/v1/matches/not-a-uuid")
                    .body(Body::empty())
                    .expect("match request should build"),
            )
            .await
            .expect("match request should complete");
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        let body = to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("error response should be readable");
        let value: serde_json::Value =
            serde_json::from_slice(&body).expect("error response should be JSON");
        assert_eq!(value["error"]["code"], "validation.invalid_match_id");
    }
}
