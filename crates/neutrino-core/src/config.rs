use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub orchestrator: OrchestratorConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrchestratorConfig {
    pub worker_count: usize,
    pub http: HttpConfig,
    pub worker: WorkerConfig,
    pub tasks: TaskConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HttpConfig {
    pub host: String,
    pub port: u16,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkerConfig {
    pub max_tasks_per_worker: u32,
    pub max_memory_mb: u64,
    pub startup_timeout_secs: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskConfig {
    pub default_timeout_secs: u64,
}

impl Config {
    /// Load configuration from YAML file
    pub fn from_file<P: AsRef<Path>>(path: P) -> Result<Self, Box<dyn std::error::Error>> {
        let content = std::fs::read_to_string(path)?;
        let config: Config = serde_yaml::from_str(&content)?;
        Ok(config)
    }

    /// Get default configuration
    pub fn default() -> Self {
        Config {
            orchestrator: OrchestratorConfig {
                worker_count: 4,
                http: HttpConfig {
                    host: "0.0.0.0".to_string(),
                    port: 8080,
                },
                worker: WorkerConfig {
                    max_tasks_per_worker: 1000,
                    max_memory_mb: 4096,
                    startup_timeout_secs: 10,
                },
                tasks: TaskConfig {
                    default_timeout_secs: 30,
                },
            },
        }
    }
}
