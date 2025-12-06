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
        let worker_pools = self.config.effective_worker_pools();
        let total_workers: usize = worker_pools.iter().map(|p| p.count).sum();

        info!("Starting orchestrator with {} workers across {} pools",
              total_workers, worker_pools.len());

        let mut workers = self.workers.write().await;

        // Spawn workers for each pool
        for pool in &worker_pools {
            info!("Spawning pool '{}': {} workers with cpus={}, gpus={}, mem={}GB",
                  pool.name, pool.count,
                  pool.resources.num_cpus, pool.resources.num_gpus, pool.resources.memory_gb);

            for pool_idx in 0..pool.count {
                let worker_id = format!("{}-{}", pool.name, pool_idx);
                info!("Spawning worker {}", worker_id);

                // Assign GPU devices for this worker
                let gpu_devices = if !pool.gpu_devices.is_empty() && pool.resources.num_gpus > 0.0 {
                    // Round-robin assignment if multiple GPUs available
                    let gpu_idx = pool_idx % pool.gpu_devices.len();
                    vec![pool.gpu_devices[gpu_idx]]
                } else {
                    vec![]
                };

                match WorkerHandle::spawn(
                    worker_id.clone(),
                    &self.config.orchestrator.app_module,
                    pool.resources.clone(),
                    &gpu_devices,
                ).await {
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

    /// Get the next available worker using round-robin selection (legacy method)
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

    /// Find a worker with sufficient resources for the given task requirements.
    /// Uses round-robin starting point but checks resource capacity.
    /// Prioritizes workers with matching resource profiles (GPU vs CPU).
    pub async fn find_worker_with_resources(
        &self,
        requirements: &crate::protocol::ResourceRequirements,
    ) -> Option<usize> {
        let workers = self.workers.read().await;
        if workers.is_empty() {
            return None;
        }

        let mut index = self.next_worker_index.write().await;
        let worker_count = workers.len();
        let start_index = *index;

        // Determine if this is a GPU task
        let is_gpu_task = requirements.num_gpus > 0.0;

        // First pass: Look for idle workers with sufficient resources
        for offset in 0..worker_count {
            let current = (start_index + offset) % worker_count;
            let worker = &workers[current].worker;

            // Skip workers that don't match resource type
            // GPU tasks should only go to GPU workers, CPU tasks prefer CPU workers
            let is_gpu_worker = worker.capabilities.num_gpus > 0.0;
            if is_gpu_task && !is_gpu_worker {
                continue; // GPU task needs GPU worker
            }

            // Check if worker is idle and has capacity
            if worker.state == WorkerState::Idle && worker.has_capacity(requirements) {
                *index = (current + 1) % worker_count;
                return Some(current);
            }
        }

        // Second pass: If no idle workers, check busy workers with capacity
        // (task will be queued, but we ensure capacity exists)
        for offset in 0..worker_count {
            let current = (start_index + offset) % worker_count;
            let worker = &workers[current].worker;

            let is_gpu_worker = worker.capabilities.num_gpus > 0.0;
            if is_gpu_task && !is_gpu_worker {
                continue;
            }

            if worker.has_capacity(requirements) {
                *index = (current + 1) % worker_count;
                return Some(current);
            }
        }

        // Third pass: If strict GPU matching failed, allow CPU-only tasks on GPU workers
        // (GPU workers can handle CPU tasks, just not optimal)
        if !is_gpu_task {
            for offset in 0..worker_count {
                let current = (start_index + offset) % worker_count;
                let worker = &workers[current].worker;

                if worker.state == WorkerState::Idle && worker.has_capacity(requirements) {
                    *index = (current + 1) % worker_count;
                    return Some(current);
                }
            }

            for offset in 0..worker_count {
                let current = (start_index + offset) % worker_count;
                let worker = &workers[current].worker;

                if worker.has_capacity(requirements) {
                    *index = (current + 1) % worker_count;
                    return Some(current);
                }
            }
        }

        // No worker has sufficient resources
        None
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
