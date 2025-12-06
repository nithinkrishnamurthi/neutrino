use neutrino_core::{AsgiManager, Config, Orchestrator};
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

    // Get config path from command-line arguments or use default
    let config_path = std::env::args().nth(1).unwrap_or_else(|| "config.yaml".to_string());

    // Load configuration
    let config = match Config::from_file(&config_path) {
        Ok(cfg) => {
            info!("Loaded configuration from {}", config_path);
            cfg
        }
        Err(e) => {
            info!("Could not load {}: {}, using defaults", config_path, e);
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
    let openapi_spec = config.orchestrator.http.openapi_spec.clone();
    let asgi_config = config.orchestrator.asgi.clone();

    // Start ASGI manager if configured in mounted mode
    let mut asgi_manager: Option<AsgiManager> = None;
    if let Some(ref asgi_cfg) = asgi_config {
        if asgi_cfg.enabled && asgi_cfg.mode == neutrino_core::config::AsgiMode::Mounted {
            info!("Starting ASGI manager in mounted mode");
            let mut manager = AsgiManager::new(asgi_cfg.clone());
            match manager.start().await {
                Ok(()) => {
                    info!("ASGI manager started successfully");
                    asgi_manager = Some(manager);
                }
                Err(e) => {
                    error!("Failed to start ASGI manager: {}", e);
                    error!("Continuing without ASGI integration");
                }
            }
        } else if asgi_cfg.enabled {
            info!("ASGI configured in proxy mode - no local process to manage");
        }
    }

    // Start HTTP server
    let server_orchestrator = Arc::clone(&orchestrator);
    let server_host = http_host.clone();
    let server_asgi_config = asgi_config.clone();
    let server_handle = tokio::spawn(async move {
        if let Err(e) = neutrino_core::http::start_server_with_openapi(
            server_orchestrator,
            server_host,
            http_port,
            openapi_spec.as_deref(),
            server_asgi_config,
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

    // Shutdown ASGI manager first (if running)
    if let Some(mut manager) = asgi_manager {
        info!("Shutting down ASGI manager");
        if let Err(e) = manager.shutdown().await {
            error!("Error shutting down ASGI manager: {}", e);
        }
    }

    // Gracefully shutdown orchestrator
    orchestrator.shutdown().await?;

    // Wait for server to finish
    server_handle.abort();

    info!("Neutrino shutdown complete");
    Ok(())
}