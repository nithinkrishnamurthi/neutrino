use serde::Deserialize;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;
use tracing::{debug, error, info, warn};

/// Configuration for backend discovery
#[derive(Debug, Clone)]
pub enum DiscoveryMode {
    /// Static list of backend URLs (for testing/simple deployments)
    Static(Vec<String>),
    // TODO: Add Kubernetes discovery in future
    // Kubernetes {
    //     namespace: String,
    //     label_selector: String,
    // },
}

/// Backend pod with resource tracking
#[derive(Debug, Clone)]
pub struct Backend {
    pub url: String,
    pub available_cpus: f64,
    pub available_gpus: f64,
    pub available_memory_gb: f64,
    pub total_cpus: f64,
    pub total_gpus: f64,
    pub total_memory_gb: f64,
    pub last_updated: Instant,
    pub healthy: bool,
    pub error_count: u32,
}

impl Backend {
    fn new(url: String) -> Self {
        Self {
            url,
            available_cpus: 0.0,
            available_gpus: 0.0,
            available_memory_gb: 0.0,
            total_cpus: 0.0,
            total_gpus: 0.0,
            total_memory_gb: 0.0,
            last_updated: Instant::now(),
            healthy: false,
            error_count: 0,
        }
    }

    /// Check if this backend has sufficient resources
    pub fn has_capacity(&self, cpus: f64, gpus: f64, memory_gb: f64) -> bool {
        self.healthy
            && self.available_cpus >= cpus
            && self.available_gpus >= gpus
            && self.available_memory_gb >= memory_gb
    }

    /// Get utilization percentage (0.0 - 1.0)
    pub fn utilization(&self) -> f64 {
        if self.total_cpus == 0.0 && self.total_gpus == 0.0 {
            return 1.0; // No capacity
        }

        let cpu_util = if self.total_cpus > 0.0 {
            1.0 - (self.available_cpus / self.total_cpus)
        } else {
            0.0
        };

        let gpu_util = if self.total_gpus > 0.0 {
            1.0 - (self.available_gpus / self.total_gpus)
        } else {
            0.0
        };

        // Return max utilization (most constrained resource)
        cpu_util.max(gpu_util)
    }
}

/// Capacity response from /capacity endpoint
#[derive(Debug, Deserialize)]
struct CapacityResponse {
    available_cpus: f64,
    available_gpus: f64,
    available_memory_gb: f64,
    #[serde(default)]
    total: Option<TotalCapacity>,
}

#[derive(Debug, Deserialize)]
struct TotalCapacity {
    cpus: f64,
    gpus: f64,
    memory_gb: f64,
}

/// Pool of backend task pods with resource tracking
pub struct BackendPool {
    backends: Arc<RwLock<Vec<Backend>>>,
    http_client: reqwest::Client,
    discovery_mode: DiscoveryMode,
    update_interval: Duration,
    capacity_timeout: Duration,
}

impl BackendPool {
    pub fn new(
        discovery_mode: DiscoveryMode,
        update_interval_secs: u64,
        capacity_timeout_secs: u64,
    ) -> Self {
        let http_client = reqwest::Client::builder()
            .timeout(Duration::from_secs(capacity_timeout_secs))
            .build()
            .expect("Failed to create HTTP client");

        Self {
            backends: Arc::new(RwLock::new(Vec::new())),
            http_client,
            discovery_mode,
            update_interval: Duration::from_secs(update_interval_secs),
            capacity_timeout: Duration::from_secs(capacity_timeout_secs),
        }
    }

    /// Initialize the pool and start background monitoring
    pub async fn start(&self) -> Result<(), String> {
        // Initialize backends based on discovery mode
        match &self.discovery_mode {
            DiscoveryMode::Static(urls) => {
                let mut backends = self.backends.write().await;
                for url in urls {
                    info!("Adding static backend: {}", url);
                    backends.push(Backend::new(url.clone()));
                }
                info!("Initialized {} static backends", backends.len());
            }
        }

        // Start background monitoring task
        self.start_monitoring().await;

        Ok(())
    }

    /// Start background task to poll backend capacities
    async fn start_monitoring(&self) {
        let backends = Arc::clone(&self.backends);
        let http_client = self.http_client.clone();
        let update_interval = self.update_interval;

        tokio::spawn(async move {
            info!(
                "Starting backend capacity monitoring (interval: {}s)",
                update_interval.as_secs()
            );

            loop {
                tokio::time::sleep(update_interval).await;

                let mut backends_guard = backends.write().await;

                for backend in backends_guard.iter_mut() {
                    match Self::fetch_capacity(&http_client, &backend.url).await {
                        Ok(capacity) => {
                            backend.available_cpus = capacity.available_cpus;
                            backend.available_gpus = capacity.available_gpus;
                            backend.available_memory_gb = capacity.available_memory_gb;

                            // Update totals if provided
                            if let Some(total) = capacity.total {
                                backend.total_cpus = total.cpus;
                                backend.total_gpus = total.gpus;
                                backend.total_memory_gb = total.memory_gb;
                            }

                            backend.last_updated = Instant::now();
                            backend.healthy = true;
                            backend.error_count = 0;

                            debug!(
                                "Backend {} capacity: CPU={:.1}/{:.1}, GPU={:.1}/{:.1}, MEM={:.1}/{:.1}GB",
                                backend.url,
                                backend.total_cpus - backend.available_cpus,
                                backend.total_cpus,
                                backend.total_gpus - backend.available_gpus,
                                backend.total_gpus,
                                backend.total_memory_gb - backend.available_memory_gb,
                                backend.total_memory_gb
                            );
                        }
                        Err(e) => {
                            backend.error_count += 1;
                            if backend.error_count >= 3 {
                                if backend.healthy {
                                    warn!("Backend {} marked unhealthy after {} errors", backend.url, backend.error_count);
                                }
                                backend.healthy = false;
                            }
                            error!("Failed to fetch capacity from {}: {}", backend.url, e);
                        }
                    }
                }
            }
        });
    }

    /// Fetch capacity from a backend
    async fn fetch_capacity(
        client: &reqwest::Client,
        backend_url: &str,
    ) -> Result<CapacityResponse, String> {
        let url = format!("{}/capacity", backend_url);

        let response = client
            .get(&url)
            .send()
            .await
            .map_err(|e| format!("Request failed: {}", e))?;

        if !response.status().is_success() {
            return Err(format!("HTTP {}", response.status()));
        }

        response
            .json::<CapacityResponse>()
            .await
            .map_err(|e| format!("Failed to parse JSON: {}", e))
    }

    /// Find a backend with sufficient resources
    /// Uses least-utilized backend among those with capacity (load balancing)
    pub async fn find_backend_with_resources(
        &self,
        cpus: f64,
        gpus: f64,
        memory_gb: f64,
    ) -> Option<Backend> {
        let backends = self.backends.read().await;

        // Find all backends with sufficient capacity
        let mut candidates: Vec<&Backend> = backends
            .iter()
            .filter(|b| b.has_capacity(cpus, gpus, memory_gb))
            .collect();

        if candidates.is_empty() {
            debug!(
                "No backends available with resources: cpus={}, gpus={}, mem={}GB",
                cpus, gpus, memory_gb
            );
            return None;
        }

        // Sort by utilization (least utilized first)
        candidates.sort_by(|a, b| {
            a.utilization()
                .partial_cmp(&b.utilization())
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        // Return least utilized backend
        let selected = candidates[0].clone();
        debug!(
            "Selected backend {} (util: {:.1}%, gpu: {:.1}/{:.1})",
            selected.url,
            selected.utilization() * 100.0,
            selected.total_gpus - selected.available_gpus,
            selected.total_gpus
        );

        Some(selected)
    }

    /// Get all backends (for monitoring/debugging)
    pub async fn get_backends(&self) -> Vec<Backend> {
        self.backends.read().await.clone()
    }

    /// Get count of healthy backends
    pub async fn healthy_count(&self) -> usize {
        self.backends.read().await.iter().filter(|b| b.healthy).count()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_backend_has_capacity() {
        let mut backend = Backend::new("http://test:8080".to_string());
        backend.available_cpus = 4.0;
        backend.available_gpus = 2.0;
        backend.available_memory_gb = 8.0;
        backend.healthy = true;

        assert!(backend.has_capacity(2.0, 1.0, 4.0));
        assert!(!backend.has_capacity(5.0, 1.0, 4.0)); // Not enough CPU
        assert!(!backend.has_capacity(2.0, 3.0, 4.0)); // Not enough GPU
        assert!(!backend.has_capacity(2.0, 1.0, 10.0)); // Not enough memory
    }

    #[test]
    fn test_backend_utilization() {
        let mut backend = Backend::new("http://test:8080".to_string());
        backend.total_cpus = 8.0;
        backend.total_gpus = 2.0;
        backend.available_cpus = 4.0; // 50% used
        backend.available_gpus = 1.0; // 50% used

        assert_eq!(backend.utilization(), 0.5);

        backend.available_gpus = 0.0; // 100% used
        assert_eq!(backend.utilization(), 1.0);
    }
}
