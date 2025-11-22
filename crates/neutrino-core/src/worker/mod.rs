use std::path::PathBuf;
use std::process::{Child, Command};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{UnixListener, UnixStream};
use tracing::{debug, error, info};

use crate::protocol::Message;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum WorkerState {
    Starting,
    Idle,
    Busy,
    Recycling,
}

#[derive(Debug)]
pub struct Worker {
    pub id: String,
    pub pid: u32,
    pub state: WorkerState,
    pub socket_path: PathBuf,
}

pub struct WorkerHandle {
    pub worker: Worker,
    pub stream: UnixStream,
    pub process: Child,
}

impl WorkerHandle {
    /// Spawn a new Python worker process and establish Unix socket connection
    pub async fn spawn(worker_id: String, app_module: &str) -> Result<Self, Box<dyn std::error::Error>> {
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

        info!("Spawning Python worker from {:?}", python_worker_path);

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

        let process = Command::new("python3")
            .arg(&python_worker_path)
            .arg(&socket_path)
            .arg(&worker_id)
            .arg(app_module)
            .env("PYTHONPATH", new_python_path)
            .current_dir(&cwd)
            .spawn()?;

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
            Message::WorkerReady { worker_id, pid } => {
                info!("Worker {} ready (pid={})", worker_id, pid);
                self.worker.state = WorkerState::Idle;
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
