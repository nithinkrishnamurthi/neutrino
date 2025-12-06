use k8s_openapi::api::core::v1::Pod;
use kube::{api::ListParams, Api, Client};
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
    /// Kubernetes service discovery
    Kubernetes {
        namespace: String,
        label_selector: String,
        port: u16,
    },
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
    available: ResourceAmounts,
    total: ResourceAmounts,
}

#[derive(Debug, Deserialize)]
struct ResourceAmounts {
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
            DiscoveryMode::Kubernetes { namespace, label_selector, port } => {
                info!(
                    "Using Kubernetes discovery: namespace={}, labels={}, port={}",
                    namespace, label_selector, port
                );

                // Discover pods immediately
                self.discover_kubernetes_backends(namespace, label_selector, *port)
                    .await
                    .map_err(|e| format!("Failed to discover Kubernetes backends: {}", e))?;

                // Start background discovery refresh
                self.start_kubernetes_discovery(namespace.clone(), label_selector.clone(), *port)
                    .await;
            }
        }

        // Start background monitoring task
        self.start_monitoring().await;

        Ok(())
    }

    /// Discover backends from Kubernetes API
    async fn discover_kubernetes_backends(
        &self,
        namespace: &str,
        label_selector: &str,
        port: u16,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let client = Client::try_default().await?;
        let pods: Api<Pod> = Api::namespaced(client, namespace);

        let lp = ListParams::default().labels(label_selector);
        let pod_list = pods.list(&lp).await?;

        let mut backends = self.backends.write().await;
        let mut new_urls = Vec::new();

        for pod in pod_list.items {
            if let Some(status) = &pod.status {
                if let Some(pod_ip) = &status.pod_ip {
                    // Only add Running pods
                    if let Some(phase) = &status.phase {
                        if phase == "Running" {
                            let url = format!("http://{}:{}", pod_ip, port);
                            new_urls.push(url.clone());

                            // Add backend if it doesn't exist
                            if !backends.iter().any(|b| b.url == url) {
                                let pod_name = pod.metadata.name.as_deref().unwrap_or("unknown");
                                info!("Discovered new backend: {} (pod: {})", url, pod_name);
                                backends.push(Backend::new(url));
                            }
                        }
                    }
                }
            }
        }

        // Remove backends that no longer exist
        backends.retain(|backend| {
            let exists = new_urls.contains(&backend.url);
            if !exists {
                info!("Removing backend (pod no longer running): {}", backend.url);
            }
            exists
        });

        info!("Kubernetes discovery complete: {} backends", backends.len());

        Ok(())
    }

    /// Start background task to periodically refresh Kubernetes pod list
    async fn start_kubernetes_discovery(&self, namespace: String, label_selector: String, port: u16) {
        let backends = Arc::clone(&self.backends);
        let update_interval = Duration::from_secs(30); // Refresh every 30 seconds

        // Clone self for use in async block
        let pool_clone = Self {
            backends,
            http_client: self.http_client.clone(),
            discovery_mode: self.discovery_mode.clone(),
            update_interval: self.update_interval,
            capacity_timeout: self.capacity_timeout,
        };

        tokio::spawn(async move {
            info!("Starting Kubernetes pod discovery (interval: 30s)");

            loop {
                tokio::time::sleep(update_interval).await;

                if let Err(e) = pool_clone
                    .discover_kubernetes_backends(&namespace, &label_selector, port)
                    .await
                {
                    error!("Failed to refresh Kubernetes backends: {}", e);
                }
            }
        });
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
                            backend.available_cpus = capacity.available.cpus;
                            backend.available_gpus = capacity.available.gpus;
                            backend.available_memory_gb = capacity.available.memory_gb;
                            backend.total_cpus = capacity.total.cpus;
                            backend.total_gpus = capacity.total.gpus;
                            backend.total_memory_gb = capacity.total.memory_gb;

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
