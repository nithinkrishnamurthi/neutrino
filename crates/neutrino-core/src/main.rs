use neutrino_core::{Config, Orchestrator};
use std::sync::Arc;
use tracing::{error, info, Level};
use tracing_subscriber;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize tracing
    tracing_subscriber::fmt()
        .with_max_level(Level::INFO)
        .init();

    info!("Starting Neutrino orchestrator");

    // Load configuration
    let config = match Config::from_file("config.yaml") {
        Ok(cfg) => {
            info!("Loaded configuration from config.yaml");
            cfg
        }
        Err(e) => {
            info!("Could not load config.yaml: {}, using defaults", e);
            Config::default()
        }
    };

    // Create orchestrator
    let orchestrator = Arc::new(Orchestrator::new(config.clone()));

    // Start worker pool
    orchestrator.start().await?;

    // Clone config values before moving into async block
    let http_host = config.orchestrator.http.host.clone();
    let http_port = config.orchestrator.http.port;

    // Start HTTP server
    let server_orchestrator = Arc::clone(&orchestrator);
    let server_host = http_host.clone();
    let server_handle = tokio::spawn(async move {
        if let Err(e) = neutrino_core::http::start_server(
            server_orchestrator,
            server_host,
            http_port,
        )
        .await
        {
            error!("HTTP server error: {}", e);
        }
    });

    info!(
        "Neutrino orchestrator running on {}:{}",
        http_host, http_port
    );

    // Wait for shutdown signal (Ctrl+C)
    tokio::signal::ctrl_c().await?;

    info!("Received shutdown signal");

    // Gracefully shutdown
    orchestrator.shutdown().await?;

    // Wait for server to finish
    server_handle.abort();

    info!("Orchestrator shutdown complete");
    Ok(())
}