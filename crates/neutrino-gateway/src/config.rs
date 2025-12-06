use std::env;

#[derive(Debug, Clone)]
pub struct GatewayConfig {
    pub port: u16,
    pub database_path: String,

    // Backend discovery
    pub discovery_mode: String,  // "static" | "kubernetes"
    pub static_backends: Vec<String>,  // Comma-separated URLs for static mode

    // Capacity monitoring
    pub capacity_update_interval_secs: u64,
    pub capacity_timeout_secs: u64,

    // OpenAPI spec for resource-aware routing
    pub openapi_spec_path: String,
}

impl GatewayConfig {
    pub fn from_env() -> Self {
        let discovery_mode = env::var("DISCOVERY_MODE")
            .unwrap_or_else(|_| "static".to_string());

        let static_backends = env::var("STATIC_BACKENDS")
            .unwrap_or_else(|_| "http://localhost:8080".to_string())
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();

        let openapi_spec_path = env::var("OPENAPI_SPEC_PATH")
            .expect("OPENAPI_SPEC_PATH environment variable is required");

        Self {
            port: env::var("GATEWAY_PORT")
                .unwrap_or_else(|_| "8080".to_string())
                .parse()
                .unwrap_or(8080),
            database_path: env::var("DATABASE_PATH")
                .unwrap_or_else(|_| "/data/neutrino.db".to_string()),
            discovery_mode,
            static_backends,
            capacity_update_interval_secs: env::var("CAPACITY_UPDATE_INTERVAL")
                .unwrap_or_else(|_| "2".to_string())
                .parse()
                .unwrap_or(2),
            capacity_timeout_secs: env::var("CAPACITY_TIMEOUT")
                .unwrap_or_else(|_| "5".to_string())
                .parse()
                .unwrap_or(5),
            openapi_spec_path,
        }
    }
}
