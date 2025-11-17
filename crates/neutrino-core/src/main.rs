use neutrino_core::worker::WorkerHandle;
use tracing::{info, Level};
use tracing_subscriber;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize tracing
    tracing_subscriber::fmt()
        .with_max_level(Level::INFO)
        .init();

    info!("Starting Neutrino orchestrator");

    // Generate worker ID
    let worker_id = format!("worker-{}", uuid::Uuid::new_v4().to_string()[..8].to_string());

    // Spawn worker
    info!("Spawning worker {}", worker_id);
    let mut worker_handle = WorkerHandle::spawn(worker_id.clone()).await?;

    // Wait for worker to be ready
    worker_handle.wait_ready().await?;

    info!("Orchestrator ready with 1 worker");

    // Keep alive for a bit then shutdown
    tokio::time::sleep(std::time::Duration::from_secs(2)).await;

    // Gracefully shutdown worker
    info!("Shutting down worker");
    worker_handle.shutdown().await?;

    info!("Orchestrator shutdown complete");
    Ok(())
}