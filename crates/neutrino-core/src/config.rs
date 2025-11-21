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
    /// Python module path for the Neutrino app (e.g., "examples.test_routes")
    pub app_module: String,
    /// Optional ASGI app configuration
    #[serde(default)]
    pub asgi: Option<AsgiConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HttpConfig {
    pub host: String,
    pub port: u16,
    #[serde(default)]
    pub openapi_spec: Option<String>,
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
                    openapi_spec: Some("openapi.json".to_string()),
                },
                worker: WorkerConfig {
                    max_tasks_per_worker: 1000,
                    max_memory_mb: 4096,
                    startup_timeout_secs: 10,
                },
                tasks: TaskConfig {
                    default_timeout_secs: 30,
                },
                app_module: "app".to_string(),
                asgi: None,
            },
        }
    }
}
