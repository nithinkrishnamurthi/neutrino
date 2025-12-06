use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;
use tokio::task::JoinHandle;
use tracing::{info, warn, debug};

use crate::config::Config;
use crate::worker::{WorkerHandle, WorkerState, memory};

/// Orchestrator manages a pool of worker processes and distributes tasks
pub struct Orchestrator {
    config: Config,
    workers: Arc<RwLock<Vec<WorkerHandle>>>,
    next_worker_index: Arc<RwLock<usize>>,
    monitoring_task: Arc<RwLock<Option<JoinHandle<()>>>>,
}

impl Orchestrator {
    /// Create a new orchestrator with the given configuration
    pub fn new(config: Config) -> Self {
        Self {
            config,
            workers: Arc::new(RwLock::new(Vec::new())),
            next_worker_index: Arc::new(RwLock::new(0)),
            monitoring_task: Arc::new(RwLock::new(None)),
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

        // Drop the write lock before starting monitoring
        drop(workers);

        // Start background memory monitoring and recycling task
        self.start_monitoring().await;

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

        // Stop monitoring task
        let mut monitoring_task = self.monitoring_task.write().await;
        if let Some(handle) = monitoring_task.take() {
            handle.abort();
            info!("Monitoring task stopped");
        }
        drop(monitoring_task);

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

    /// Start background memory monitoring and worker recycling task
    async fn start_monitoring(&self) {
        let workers = Arc::clone(&self.workers);
        let config = self.config.clone();
        let check_interval = Duration::from_secs(config.orchestrator.worker.memory_check_interval_secs);

        info!(
            "Starting memory monitoring task (interval: {} seconds)",
            check_interval.as_secs()
        );

        let handle = tokio::spawn(async move {
            loop {
                tokio::time::sleep(check_interval).await;

                let mut workers_guard = workers.write().await;
                let mut workers_to_recycle = Vec::new();

                // Check each worker's memory and recycling thresholds
                for (idx, worker_handle) in workers_guard.iter_mut().enumerate() {
                    let worker = &mut worker_handle.worker;

                    // Update memory usage
                    match memory::get_process_memory_mb(worker.pid) {
                        Ok(memory_mb) => {
                            worker.update_memory(memory_mb);
                            debug!(
                                "Worker {} memory: {} MB (tasks: {}, lifetime: {}s)",
                                worker.id,
                                memory_mb,
                                worker.tasks_completed,
                                worker.spawn_time.elapsed().as_secs()
                            );
                        }
                        Err(e) => {
                            warn!("Failed to get memory for worker {}: {}", worker.id, e);
                            continue;
                        }
                    }

                    // Check if worker should be recycled
                    if worker.should_recycle(&config.orchestrator.worker) {
                        // Only recycle idle workers to avoid interrupting tasks
                        if worker.state == WorkerState::Idle {
                            info!(
                                "Worker {} marked for recycling (tasks: {}, memory: {} MB, lifetime: {}s)",
                                worker.id,
                                worker.tasks_completed,
                                worker.current_memory_mb,
                                worker.spawn_time.elapsed().as_secs()
                            );
                            workers_to_recycle.push(idx);
                        } else {
                            debug!(
                                "Worker {} needs recycling but is busy, deferring",
                                worker.id
                            );
                        }
                    }
                }

                // Recycle workers (in reverse order to maintain indices)
                for &idx in workers_to_recycle.iter().rev() {
                    if let Err(e) = Self::recycle_worker_at_index(&mut workers_guard, idx, &config).await {
                        warn!("Failed to recycle worker at index {}: {}", idx, e);
                    }
                }
            }
        });

        let mut monitoring_task = self.monitoring_task.write().await;
        *monitoring_task = Some(handle);
    }

    /// Recycle a worker at a specific index
    async fn recycle_worker_at_index(
        workers: &mut Vec<WorkerHandle>,
        idx: usize,
        config: &crate::config::Config,
    ) -> Result<(), String> {
        if idx >= workers.len() {
            return Err("Invalid worker index".into());
        }

        // Get the worker to be recycled
        let old_worker = workers.remove(idx);
        let worker_id = old_worker.worker.id.clone();
        let pool_name = worker_id.split('-').next().unwrap_or("default");

        info!("Recycling worker {}", worker_id);

        // Find the pool configuration for this worker
        let worker_pools = config.effective_worker_pools();
        let pool = worker_pools
            .iter()
            .find(|p| p.name == pool_name)
            .ok_or_else(|| format!("Pool {} not found", pool_name))?;

        // Gracefully shutdown old worker
        let mut old_worker = old_worker;
        if let Err(e) = old_worker.shutdown().await {
            warn!("Error shutting down old worker {}: {}", worker_id, e);
        }

        // Extract the pool index from the worker ID (e.g., "default-1" -> 1)
        let pool_idx: usize = worker_id
            .split('-')
            .last()
            .and_then(|s| s.parse().ok())
            .unwrap_or(0);

        // Determine GPU devices for the new worker
        let gpu_devices = if !pool.gpu_devices.is_empty() && pool.resources.num_gpus > 0.0 {
            let gpu_idx = pool_idx % pool.gpu_devices.len();
            vec![pool.gpu_devices[gpu_idx]]
        } else {
            vec![]
        };

        // Spawn replacement worker with same configuration
        info!("Spawning replacement worker {}", worker_id);
        let mut new_worker = match WorkerHandle::spawn(
            worker_id.clone(),
            &config.orchestrator.app_module,
            pool.resources.clone(),
            &gpu_devices,
        )
        .await
        {
            Ok(worker) => worker,
            Err(e) => {
                let err_msg = format!("Failed to spawn replacement worker {}: {}", worker_id, e);
                warn!("{}", err_msg);
                return Err(err_msg);
            }
        };

        // Wait for worker to be ready
        match new_worker.wait_ready().await {
            Ok(()) => {
                info!("Replacement worker {} is ready", worker_id);
                workers.insert(idx, new_worker);
                Ok(())
            }
            Err(e) => {
                let err_msg = format!("Replacement worker {} failed to become ready: {}", worker_id, e);
                warn!("{}", err_msg);
                Err(err_msg)
            }
        }
    }
}
