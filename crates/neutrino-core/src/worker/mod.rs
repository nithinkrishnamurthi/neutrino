use std::path::PathBuf;
use std::process::{Child, Command};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{UnixListener, UnixStream};
use tracing::{debug, error, info};
use serde::{Deserialize, Serialize};

use crate::protocol::{Message, ResourceCapabilities};

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum WorkerState {
    Starting,
    Idle,
    Busy,
    Recycling,
}

/// Current resource allocation state of a worker
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceAllocation {
    /// CPUs currently allocated
    pub allocated_cpus: f64,
    /// GPUs currently allocated
    pub allocated_gpus: f64,
    /// Memory currently allocated in GB
    pub allocated_memory_gb: f64,
}

impl Default for ResourceAllocation {
    fn default() -> Self {
        Self {
            allocated_cpus: 0.0,
            allocated_gpus: 0.0,
            allocated_memory_gb: 0.0,
        }
    }
}

impl ResourceAllocation {
    /// Allocate resources for a task
    pub fn allocate(&mut self, requirements: &crate::protocol::ResourceRequirements) {
        self.allocated_cpus += requirements.num_cpus;
        self.allocated_gpus += requirements.num_gpus;
        self.allocated_memory_gb += requirements.memory_gb;
    }

    /// Deallocate resources after task completion
    pub fn deallocate(&mut self, requirements: &crate::protocol::ResourceRequirements) {
        self.allocated_cpus -= requirements.num_cpus;
        self.allocated_gpus -= requirements.num_gpus;
        self.allocated_memory_gb -= requirements.memory_gb;

        // Ensure no negative values due to floating point precision
        self.allocated_cpus = self.allocated_cpus.max(0.0);
        self.allocated_gpus = self.allocated_gpus.max(0.0);
        self.allocated_memory_gb = self.allocated_memory_gb.max(0.0);
    }
}

#[derive(Debug)]
pub struct Worker {
    pub id: String,
    pub pid: u32,
    pub state: WorkerState,
    pub socket_path: PathBuf,
    /// Total resource capabilities of this worker
    pub capabilities: ResourceCapabilities,
    /// Current resource allocation
    pub allocation: ResourceAllocation,
}

impl Worker {
    /// Check if this worker has sufficient available resources for a task
    pub fn has_capacity(&self, requirements: &crate::protocol::ResourceRequirements) -> bool {
        let available_cpus = self.capabilities.num_cpus - self.allocation.allocated_cpus;
        let available_gpus = self.capabilities.num_gpus - self.allocation.allocated_gpus;
        let available_memory_gb = self.capabilities.memory_gb - self.allocation.allocated_memory_gb;

        available_cpus >= requirements.num_cpus
            && available_gpus >= requirements.num_gpus
            && available_memory_gb >= requirements.memory_gb
    }

    /// Get available resources as a tuple (cpus, gpus, memory_gb)
    pub fn available_resources(&self) -> (f64, f64, f64) {
        (
            self.capabilities.num_cpus - self.allocation.allocated_cpus,
            self.capabilities.num_gpus - self.allocation.allocated_gpus,
            self.capabilities.memory_gb - self.allocation.allocated_memory_gb,
        )
    }
}

pub struct WorkerHandle {
    pub worker: Worker,
    pub stream: UnixStream,
    pub process: Child,
}

impl WorkerHandle {
    /// Spawn a new Python worker process and establish Unix socket connection
    pub async fn spawn(
        worker_id: String,
        app_module: &str,
        capabilities: ResourceCapabilities,
        gpu_devices: &[usize],
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let socket_path = PathBuf::from(format!("/tmp/neutrino-{}.sock", worker_id));

        // Clean up old socket if it exists
        if socket_path.exists() {
            std::fs::remove_file(&socket_path)?;
        }

        // Create Unix socket listener
        let listener = UnixListener::bind(&socket_path)?;
        info!("Created socket at {:?}", socket_path);

        // Spawn Python worker process
        // When running from workspace root (/home/nithin/neutrino), path is python/neutrino/internal/worker/...
        let python_worker_path = std::env::current_dir()?
            .join("python")
            .join("neutrino")
            .join("internal")
            .join("worker")
            .join("main.py");

        info!(
            "Spawning Python worker from {:?} with resources: cpus={}, gpus={}, mem={}GB",
            python_worker_path, capabilities.num_cpus, capabilities.num_gpus, capabilities.memory_gb
        );

        // Get the current working directory to add to PYTHONPATH
        let cwd = std::env::current_dir()?;
        let python_path = std::env::var("PYTHONPATH").unwrap_or_default();

        // Add both the cwd and python/ directory to PYTHONPATH
        let python_dir = cwd.join("python");
        let new_python_path = if python_path.is_empty() {
            format!("{}:{}", cwd.display(), python_dir.display())
        } else {
            format!("{}:{}:{}", python_path, cwd.display(), python_dir.display())
        };

        // Build command with environment variables
        let mut cmd = Command::new("python3");
        cmd.arg(&python_worker_path)
            .arg(&socket_path)
            .arg(&worker_id)
            .arg(app_module)
            .arg(capabilities.num_cpus.to_string())
            .arg(capabilities.num_gpus.to_string())
            .arg(capabilities.memory_gb.to_string())
            .env("PYTHONPATH", new_python_path)
            .current_dir(&cwd);

        // Set CUDA_VISIBLE_DEVICES for GPU isolation
        if !gpu_devices.is_empty() {
            let cuda_devices = gpu_devices
                .iter()
                .map(|d| d.to_string())
                .collect::<Vec<_>>()
                .join(",");
            info!("Worker {} using CUDA_VISIBLE_DEVICES={}", worker_id, cuda_devices);
            cmd.env("CUDA_VISIBLE_DEVICES", &cuda_devices);
        } else if capabilities.num_gpus == 0.0 {
            // Explicitly hide all GPUs for CPU-only workers
            info!("Worker {} is CPU-only (CUDA_VISIBLE_DEVICES='')", worker_id);
            cmd.env("CUDA_VISIBLE_DEVICES", "");
        }

        let process = cmd.spawn()?;

        let pid = process.id();
        info!("Worker {} spawned with PID {}", worker_id, pid);

        // Wait for worker to connect (with timeout)
        info!("Waiting for worker to connect...");
        let (stream, _addr) = tokio::time::timeout(
            std::time::Duration::from_secs(10),
            listener.accept(),
        )
        .await??;

        info!("Worker {} connected", worker_id);

        let worker = Worker {
            id: worker_id,
            pid,
            state: WorkerState::Starting,
            socket_path,
            capabilities,
            allocation: ResourceAllocation::default(),
        };

        Ok(Self {
            worker,
            stream,
            process,
        })
    }

    /// Send a message to the worker
    pub async fn send(&mut self, msg: &Message) -> Result<(), Box<dyn std::error::Error>> {
        let payload = msg.to_bytes()?;
        let len = (payload.len() as u32).to_be_bytes();

        self.stream.write_all(&len).await?;
        self.stream.write_all(&payload).await?;
        self.stream.flush().await?;

        debug!("Sent message: {:?}", msg);
        Ok(())
    }

    /// Receive a message from the worker
    pub async fn recv(&mut self) -> Result<Message, Box<dyn std::error::Error>> {
        let mut len_buf = [0u8; 4];
        self.stream.read_exact(&mut len_buf).await?;
        let len = u32::from_be_bytes(len_buf) as usize;

        let mut payload = vec![0u8; len];
        self.stream.read_exact(&mut payload).await?;

        let msg = Message::from_bytes(&payload)?;
        debug!("Received message: {:?}", msg);
        Ok(msg)
    }

    /// Wait for the worker to send a Ready message
    pub async fn wait_ready(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        match self.recv().await? {
            Message::WorkerReady { worker_id, pid, capabilities } => {
                info!(
                    "Worker {} ready (pid={}, cpus={}, gpus={}, mem={}GB)",
                    worker_id, pid, capabilities.num_cpus, capabilities.num_gpus, capabilities.memory_gb
                );
                self.worker.state = WorkerState::Idle;
                self.worker.capabilities = capabilities;
                Ok(())
            }
            other => {
                error!("Expected WorkerReady, got {:?}", other);
                Err("Unexpected message".into())
            }
        }
    }

    /// Gracefully shutdown the worker
    pub async fn shutdown(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        self.send(&Message::Shutdown { graceful: true }).await?;
        self.process.wait()?;

        // Clean up socket
        if self.worker.socket_path.exists() {
            std::fs::remove_file(&self.worker.socket_path)?;
        }

        Ok(())
    }
}

impl Drop for WorkerHandle {
    fn drop(&mut self) {
        // Clean up socket file on drop
        if self.worker.socket_path.exists() {
            let _ = std::fs::remove_file(&self.worker.socket_path);
        }
    }
}
