#![forbid(unsafe_code)]

use football_infrastructure::{DatabasePoolConfig, connect_database_lazy};
use football_platform::{ApplicationConfig, init_logging};
use std::error::Error;
use tokio::net::TcpListener;
use tracing::info;

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
    let app = football_admin_api::build_router(database);
    let listener = TcpListener::bind(address).await?;

    info!(
        service = "football-admin-api",
        bind_address = %address,
        route = football_contracts::ADMIN_API_PREFIX,
        "admin API listening"
    );
    axum::serve(listener, app).await?;
    Ok(())
}
