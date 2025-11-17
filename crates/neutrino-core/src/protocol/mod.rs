use serde::{Deserialize, Serialize};

/// Messages exchanged between orchestrator and workers via Unix socket.
/// Wire format: [4 bytes: big-endian length][N bytes: msgpack payload]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Message {
    /// Worker announces it's ready to receive tasks
    WorkerReady { worker_id: String, pid: u32 },

    /// Orchestrator assigns a task to a worker
    TaskAssignment {
        task_id: String,
        function_name: String,
        args: Vec<u8>, // msgpack-encoded arguments
    },

    /// Worker reports task completion
    TaskResult {
        task_id: String,
        success: bool,
        result: Vec<u8>, // msgpack-encoded result or error
    },

    /// Orchestrator requests worker shutdown
    Shutdown { graceful: bool },

    /// Heartbeat for health checking
    Heartbeat { worker_id: String },

    RouteRegistry { routes: Vec<String, Vec<String>> },
    
    DiscoverRoutes,
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
