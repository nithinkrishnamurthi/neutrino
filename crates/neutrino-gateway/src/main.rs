mod config;
mod db_logger;
mod proxy;

use axum::{routing::any, Router};
use std::sync::Arc;
use tracing::{info, Level};
use tracing_subscriber;

use crate::config::GatewayConfig;
use crate::db_logger::DbLogger;
use crate::proxy::{proxy_handler, AppState};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize tracing
    tracing_subscriber::fmt().with_max_level(Level::INFO).init();

    info!("Starting Neutrino Gateway");

    // Load configuration
    let config = GatewayConfig::from_env();

    info!("Configuration:");
    info!("  Port: {}", config.port);
    info!("  Backend URL: {}", config.backend_url);
    info!("  Database path: {}", config.database_path);

    // Initialize database logger
    let db_logger = Arc::new(DbLogger::new(config.database_path.clone()));

    // Create HTTP client for proxying
    let http_client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(300)) // 5 minute timeout
        .build()?;

    // Create app state
    let state = AppState {
        backend_url: config.backend_url.clone(),
        http_client,
        db_logger,
    };

    // Create router - catch all requests and proxy them
    let app = Router::new()
        .fallback(any(proxy_handler))
        .with_state(state);

    // Start server
    let addr = format!("0.0.0.0:{}", config.port);
    info!("Gateway listening on {}", addr);

    let listener = tokio::net::TcpListener::bind(&addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}
