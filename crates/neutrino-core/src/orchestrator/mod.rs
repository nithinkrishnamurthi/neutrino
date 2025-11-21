use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{info, warn};

use crate::config::Config;
use crate::worker::{WorkerHandle, WorkerState};

/// Orchestrator manages a pool of worker processes and distributes tasks
pub struct Orchestrator {
    config: Config,
    workers: Arc<RwLock<Vec<WorkerHandle>>>,
    next_worker_index: Arc<RwLock<usize>>,
}

impl Orchestrator {
    /// Create a new orchestrator with the given configuration
    pub fn new(config: Config) -> Self {
        Self {
            config,
            workers: Arc::new(RwLock::new(Vec::new())),
            next_worker_index: Arc::new(RwLock::new(0)),
        }
    }

    /// Start the orchestrator by spawning all worker processes
    pub async fn start(&self) -> Result<(), Box<dyn std::error::Error>> {
        info!(
            "Starting orchestrator with {} workers",
            self.config.orchestrator.worker_count
        );

        let mut workers = self.workers.write().await;

        for i in 0..self.config.orchestrator.worker_count {
            let worker_id = format!("worker-{}", i);
            info!("Spawning {}", worker_id);

            match WorkerHandle::spawn(worker_id.clone(), &self.config.orchestrator.app_module).await {
                Ok(mut handle) => {
                    // Wait for worker to be ready
                    if let Err(e) = handle.wait_ready().await {
                        warn!("Worker {} failed to become ready: {}", worker_id, e);
                        continue;
                    }
                    info!("Worker {} is ready", worker_id);
                    workers.push(handle);
                }
                Err(e) => {
                    warn!("Failed to spawn worker {}: {}", worker_id, e);
                }
            }
        }

        info!(
            "Orchestrator started with {} active workers",
            workers.len()
        );

        if workers.is_empty() {
            return Err("No workers could be started".into());
        }

        Ok(())
    }

    /// Get the next available worker using round-robin selection
    pub async fn get_next_worker(&self) -> Option<usize> {
        let workers = self.workers.read().await;
        if workers.is_empty() {
            return None;
        }

        let mut index = self.next_worker_index.write().await;
        let worker_count = workers.len();

        // Find next idle worker using round-robin
        for _ in 0..worker_count {
            let current = *index;
            *index = (*index + 1) % worker_count;

            if workers[current].worker.state == WorkerState::Idle {
                return Some(current);
            }
        }

        // If no idle workers, just return the next in round-robin
        // (worker will queue the task)
        let current = *index;
        *index = (*index + 1) % worker_count;
        Some(current)
    }

    /// Get a reference to the worker pool
    pub fn workers(&self) -> Arc<RwLock<Vec<WorkerHandle>>> {
        Arc::clone(&self.workers)
    }

    /// Get the number of active workers
    pub async fn worker_count(&self) -> usize {
        self.workers.read().await.len()
    }

    /// Shutdown all workers gracefully
    pub async fn shutdown(&self) -> Result<(), Box<dyn std::error::Error>> {
        info!("Shutting down orchestrator");
        let mut workers = self.workers.write().await;

        for worker in workers.iter_mut() {
            info!("Shutting down worker {}", worker.worker.id);
            if let Err(e) = worker.shutdown().await {
                warn!("Error shutting down worker {}: {}", worker.worker.id, e);
            }
        }

        workers.clear();
        info!("All workers shut down");
        Ok(())
    }
}
