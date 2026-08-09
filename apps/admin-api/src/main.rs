#![forbid(unsafe_code)]

mod health;

use axum::{Json, Router, middleware, routing::get};
use football_contracts::{ADMIN_API_MAJOR_VERSION, ADMIN_API_PREFIX};
use football_infrastructure::{DatabasePoolConfig, connect_database_lazy};
use football_platform::{ApplicationConfig, init_logging, request_id_middleware};
use serde::Serialize;
use std::error::Error;
use tokio::net::TcpListener;
use tracing::info;

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

#[tokio::main]
async fn main() {
    if let Err(error) = run().await {
        eprintln!("football-admin-api failed: {error}");
        std::process::exit(1);
    }
}

async fn run() -> Result<(), Box<dyn Error + Send + Sync>> {
    let config = ApplicationConfig::from_env()?;
    init_logging(config.log_filter())?;
    let address = config.admin_api_bind_addr();
    let database = connect_database_lazy(config.database_url(), DatabasePoolConfig::default())?;
    let app = Router::new()
        .route(health::LIVENESS_PATH, get(health::liveness))
        .route(health::READINESS_PATH, get(health::readiness))
        .route(ADMIN_API_PREFIX, get(api_info))
        .with_state(database)
        .layer(middleware::from_fn(request_id_middleware));
    let listener = TcpListener::bind(address).await?;

    info!(
        service = "football-admin-api",
        bind_address = %address,
        route = ADMIN_API_PREFIX,
        "admin API listening"
    );
    axum::serve(listener, app).await?;
    Ok(())
}
