use serde::{Deserialize, Serialize};

/// Resource requirements for a task
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ResourceRequirements {
    /// CPUs required (logical cores, can be fractional)
    pub num_cpus: f64,
    /// GPUs required (devices, can be fractional)
    pub num_gpus: f64,
    /// Memory required in GB
    pub memory_gb: f64,
}

impl Default for ResourceRequirements {
    fn default() -> Self {
        Self {
            num_cpus: 1.0,
            num_gpus: 0.0,
            memory_gb: 1.0,
        }
    }
}

/// Resource capabilities of a worker
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ResourceCapabilities {
    /// Total CPUs available (logical cores, can be fractional)
    pub num_cpus: f64,
    /// Total GPUs available (devices, can be fractional for sharing)
    pub num_gpus: f64,
    /// Total memory in GB
    pub memory_gb: f64,
}

impl Default for ResourceCapabilities {
    fn default() -> Self {
        Self {
            num_cpus: 1.0,
            num_gpus: 0.0,
            memory_gb: 4.0,
        }
    }
}

/// Messages exchanged between orchestrator and workers via Unix socket.
/// Wire format: [4 bytes: big-endian length][N bytes: msgpack payload]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Message {
    /// Worker announces it's ready to receive tasks
    WorkerReady {
        worker_id: String,
        pid: u32,
        capabilities: ResourceCapabilities,
    },

    /// Orchestrator assigns a task to a worker
    TaskAssignment {
        task_id: String,
        function_name: String,
        args: rmpv::Value, // Native msgpack value (encoded once with entire message)
        resources: ResourceRequirements,
    },

    /// Worker reports task completion
    TaskResult {
        task_id: String,
        success: bool,
        result: rmpv::Value, // Native msgpack value (encoded once with entire message)
    },

    /// Orchestrator requests worker shutdown
    Shutdown { graceful: bool },

    /// Heartbeat for health checking
    Heartbeat { worker_id: String },
}

impl Message {
    /// Serialize message to msgpack bytes
    pub fn to_bytes(&self) -> Result<Vec<u8>, rmp_serde::encode::Error> {
        rmp_serde::to_vec(self)
    }

    /// Deserialize message from msgpack bytes
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, rmp_serde::decode::Error> {
        rmp_serde::from_slice(bytes)
    }
}
