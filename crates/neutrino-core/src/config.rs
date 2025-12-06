use serde::{Deserialize, Serialize};
use std::path::Path;
use crate::protocol::ResourceCapabilities;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub orchestrator: OrchestratorConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrchestratorConfig {
    /// Total worker count (deprecated, use worker_pools instead)
    #[serde(default)]
    pub worker_count: Option<usize>,
    pub http: HttpConfig,
    pub worker: WorkerConfig,
    pub tasks: TaskConfig,
    /// Python module path for the Neutrino app (e.g., "examples.test_routes")
    pub app_module: String,
    /// Optional ASGI app configuration
    #[serde(default)]
    pub asgi: Option<AsgiConfig>,
    /// Worker pools with different resource configurations
    #[serde(default)]
    pub worker_pools: Vec<WorkerPoolConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HttpConfig {
    pub host: String,
    pub port: u16,
    #[serde(default)]
    pub openapi_spec: Option<String>,
}

/// Configuration for a specific pool of workers
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkerPoolConfig {
    /// Name of the worker pool (e.g., "gpu_workers", "cpu_workers")
    pub name: String,
    /// Number of workers in this pool
    pub count: usize,
    /// Resource capabilities of each worker in this pool
    pub resources: ResourceCapabilities,
    /// GPU device indices to use (e.g., [0, 1] for GPUs 0 and 1)
    #[serde(default)]
    pub gpu_devices: Vec<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkerConfig {
    /// Maximum number of tasks a worker can execute before being recycled
    pub max_tasks_per_worker: u32,
    /// Maximum memory usage in MB before recycling
    pub max_memory_mb: u64,
    /// Maximum worker lifetime in seconds before recycling
    #[serde(default = "default_max_lifetime_secs")]
    pub max_lifetime_secs: u64,
    /// Interval in seconds for checking worker memory usage
    #[serde(default = "default_memory_check_interval_secs")]
    pub memory_check_interval_secs: u64,
    /// Worker startup timeout
    pub startup_timeout_secs: u64,
}

fn default_max_lifetime_secs() -> u64 {
    3600 // 1 hour
}

fn default_memory_check_interval_secs() -> u64 {
    30 // Check every 30 seconds
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskConfig {
    pub default_timeout_secs: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AsgiConfig {
    /// Whether ASGI integration is enabled
    pub enabled: bool,
    /// Deployment mode: "mounted" (same process) or "proxy" (separate service)
    pub mode: AsgiMode,
    /// Port for internal Uvicorn server (mounted mode only)
    #[serde(default = "default_asgi_port")]
    pub port: u16,
    /// Number of Uvicorn workers (mounted mode only)
    #[serde(default = "default_asgi_workers")]
    pub workers: usize,
    /// External service URL (proxy mode only)
    #[serde(default)]
    pub service_url: Option<String>,
    /// Request timeout in seconds for proxied requests
    #[serde(default = "default_asgi_timeout")]
    pub timeout_secs: u64,
    /// Uvicorn app command (e.g., "uvicorn_app:app" or "myapp:application")
    #[serde(default = "default_asgi_app_command")]
    pub app_command: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum AsgiMode {
    Mounted,
    Proxy,
}

fn default_asgi_port() -> u16 {
    8081
}

fn default_asgi_workers() -> usize {
    4
}

fn default_asgi_timeout() -> u64 {
    30
}

fn default_asgi_app_command() -> String {
    "uvicorn_app:app".to_string()
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
                worker_count: Some(4),
                http: HttpConfig {
                    host: "0.0.0.0".to_string(),
                    port: 8080,
                    openapi_spec: Some("openapi.json".to_string()),
                },
                worker: WorkerConfig {
                    max_tasks_per_worker: 1000,
                    max_memory_mb: 4096,
                    max_lifetime_secs: 3600,
                    memory_check_interval_secs: 30,
                    startup_timeout_secs: 10,
                },
                tasks: TaskConfig {
                    default_timeout_secs: 30,
                },
                app_module: "app".to_string(),
                asgi: None,
                worker_pools: vec![],
            },
        }
    }

    /// Get the effective worker count (either from worker_pools or legacy worker_count)
    pub fn effective_worker_count(&self) -> usize {
        if !self.orchestrator.worker_pools.is_empty() {
            self.orchestrator.worker_pools.iter().map(|p| p.count).sum()
        } else {
            self.orchestrator.worker_count.unwrap_or(4)
        }
    }

    /// Get worker pools, creating a default pool if none specified
    pub fn effective_worker_pools(&self) -> Vec<WorkerPoolConfig> {
        if !self.orchestrator.worker_pools.is_empty() {
            self.orchestrator.worker_pools.clone()
        } else {
            // Create default pool from legacy worker_count
            vec![WorkerPoolConfig {
                name: "default".to_string(),
                count: self.orchestrator.worker_count.unwrap_or(4),
                resources: ResourceCapabilities::default(),
                gpu_devices: vec![],
            }]
        }
    }
}
