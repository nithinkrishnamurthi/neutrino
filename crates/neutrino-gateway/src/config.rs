use std::env;

#[derive(Debug, Clone)]
pub struct GatewayConfig {
    pub port: u16,
    pub backend_url: String,
    pub database_path: String,
}

impl GatewayConfig {
    pub fn from_env() -> Self {
        Self {
            port: env::var("GATEWAY_PORT")
                .unwrap_or_else(|_| "8080".to_string())
                .parse()
                .unwrap_or(8080),
            backend_url: env::var("BACKEND_URL")
                .unwrap_or_else(|_| "http://neutrino:8080".to_string()),
            database_path: env::var("DATABASE_PATH")
                .unwrap_or_else(|_| "/data/neutrino.db".to_string()),
        }
    }
}
