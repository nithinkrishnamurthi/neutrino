# Gateway Resource-Aware Routing Implementation Plan

## Problem Statement

Current gateway is a simple proxy that forwards all requests to a single backend URL (`http://neutrino:8080`), which resolves to a Kubernetes Service that load balances randomly across task pods. This can route GPU-requiring tasks to pods without available GPUs, causing 503 errors and requiring client retries.

## Solution: Intelligent Resource-Aware Routing

Transform `neutrino-gateway` into an intelligent router that:
- Tracks multiple task pod endpoints
- Monitors each pod's resource availability (GPUs, CPUs, memory)
- Routes requests based on task resource requirements
- Provides graceful degradation when no capacity available

---

## Architecture

### Current (Dumb Proxy):
```
[Client] → [Gateway] → [K8s Service] → [Random Pod]
            ↓                              ↓
        SQLite Log                   May not have GPUs!
```

### Proposed (Smart Router):
```
[Client] → [Gateway] → [Selected Pod with Resources]
            ↓              ↑
        SQLite Log    Capacity tracking
            ↓              ↓
      [Backend Pool]   [Pod 1: GPU=2, CPU=8]
      Capacity Cache   [Pod 2: GPU=0, CPU=8]
                       [Pod 3: GPU=1, CPU=4]
```

---

## Implementation Steps

### Phase 1: Capacity Reporting (neutrino-core)

**File**: `crates/neutrino-core/src/http/mod.rs`

**Add endpoint**: `GET /capacity`
```rust
async fn get_capacity_detailed(State(state): State<AppState>) -> impl IntoResponse {
    let workers = state.orchestrator.workers();
    let workers_guard = workers.read().await;

    let mut available_cpus = 0.0;
    let mut available_gpus = 0.0;
    let mut available_memory_gb = 0.0;

    for worker_handle in workers_guard.iter() {
        if worker_handle.worker.state == WorkerState::Idle {
            let (cpu, gpu, mem) = worker_handle.worker.available_resources();
            available_cpus += cpu;
            available_gpus += gpu;
            available_memory_gb += mem;
        }
    }

    Json(serde_json::json!({
        "available_cpus": available_cpus,
        "available_gpus": available_gpus,
        "available_memory_gb": available_memory_gb,
        "total_workers": workers_guard.len(),
        "idle_workers": workers_guard.iter().filter(|w| w.worker.state == WorkerState::Idle).count(),
    }))
}
```

**Register route**: Add to router in `create_router_with_openapi()`

---

### Phase 2: Backend Pool (neutrino-gateway)

**File**: `crates/neutrino-gateway/src/backend_pool.rs` (NEW)

**Key structures**:
```rust
pub struct BackendPool {
    backends: Arc<RwLock<Vec<Backend>>>,
    discovery_mode: DiscoveryMode,
    http_client: reqwest::Client,
}

pub struct Backend {
    pub url: String,
    pub available_cpus: f64,
    pub available_gpus: f64,
    pub available_memory_gb: f64,
    pub last_updated: Instant,
    pub healthy: bool,
}

pub enum DiscoveryMode {
    Static(Vec<String>),  // For testing
    Kubernetes {          // For production
        namespace: String,
        label_selector: String,
    },
}
```

**Key methods**:
- `new()` - Initialize pool
- `start_monitoring()` - Background task to poll `/capacity` endpoints
- `find_backend_with_resources(cpus, gpus, mem)` - Select backend with capacity
- `discover_backends()` - For k8s mode, query API for pods

---

### Phase 3: Smart Routing (neutrino-gateway)

**File**: `crates/neutrino-gateway/src/proxy.rs`

**Modify `proxy_handler()`**:
```rust
pub async fn proxy_handler(
    State(state): State<AppState>,
    req: Request<Body>,
) -> Result<Response<Body>, ProxyError> {
    // ... existing logging code ...

    // NEW: Extract resource requirements from request
    let resources = extract_resource_requirements(&path, &state.openapi_spec);

    // NEW: Find backend with capacity
    let backend = state.backend_pool
        .find_backend_with_resources(
            resources.num_cpus,
            resources.num_gpus,
            resources.memory_gb
        )
        .await
        .ok_or_else(|| ProxyError::NoCapacity)?;

    // Build target URL (replace single backend_url)
    let target_url = format!("{}{}{}", backend.url, path, query);

    // ... rest of existing proxy logic ...
}
```

**Add resource extraction**:
```rust
fn extract_resource_requirements(path: &str, spec: &OpenApiSpec) -> ResourceRequirements {
    // Parse OpenAPI spec to find route's resource requirements
    // Fallback to defaults if not found
}
```

---

### Phase 4: Configuration (neutrino-gateway)

**File**: `crates/neutrino-gateway/src/config.rs`

**Update `GatewayConfig`**:
```rust
pub struct GatewayConfig {
    pub port: u16,
    pub database_path: String,

    // NEW: Backend discovery
    pub discovery_mode: String,  // "static" | "kubernetes"
    pub static_backends: Vec<String>,  // For static mode
    pub k8s_namespace: String,  // For k8s mode
    pub k8s_label_selector: String,  // e.g., "app=neutrino-task"

    // NEW: Capacity monitoring
    pub capacity_update_interval_secs: u64,  // Default: 2
    pub capacity_timeout_secs: u64,  // Default: 5

    // NEW: OpenAPI spec for resource extraction
    pub openapi_spec_path: Option<String>,
}
```

**Environment variables**:
- `DISCOVERY_MODE` - "static" or "kubernetes"
- `STATIC_BACKENDS` - Comma-separated URLs
- `K8S_NAMESPACE` - Kubernetes namespace
- `K8S_LABEL_SELECTOR` - Label to find task pods
- `CAPACITY_UPDATE_INTERVAL` - Polling interval (seconds)
- `OPENAPI_SPEC_PATH` - Path to OpenAPI JSON

---

### Phase 5: Kubernetes Discovery (neutrino-gateway)

**File**: `crates/neutrino-gateway/src/k8s_discovery.rs` (NEW)

**Dependencies**: Add to `Cargo.toml`:
```toml
kube = "0.87"
k8s-openapi = { version = "0.20", features = ["v1_28"] }
```

**Implementation**:
```rust
pub async fn discover_pods(namespace: &str, label_selector: &str) -> Result<Vec<String>, Error> {
    let client = kube::Client::try_default().await?;
    let pods: Api<Pod> = Api::namespaced(client, namespace);

    let lp = ListParams::default().labels(label_selector);
    let pod_list = pods.list(&lp).await?;

    let urls: Vec<String> = pod_list
        .items
        .iter()
        .filter_map(|pod| {
            pod.status.as_ref()
                .and_then(|s| s.pod_ip.as_ref())
                .map(|ip| format!("http://{}:8080", ip))
        })
        .collect();

    Ok(urls)
}
```

---

## Testing Strategy

### Unit Tests
- Backend pool selection logic
- Resource requirement extraction
- Health check handling

### Integration Tests
1. **Static mode**: Configure 3 backends with different capacities
2. **Route GPU task**: Should select backend with GPU
3. **Route CPU task**: Should select any backend
4. **No capacity**: Should return 503

### Load Tests
- 1000 req/s with mixed GPU/CPU tasks
- Verify proper distribution
- Measure latency overhead (target: <5ms)

---

## Rollout Plan

### v0.2-alpha (Testing)
- Static backend mode only
- Manual configuration of backend URLs
- Capacity polling every 5 seconds

### v0.2-beta (Pre-production)
- Add Kubernetes discovery
- Reduce polling to 2 seconds
- Add metrics/observability

### v0.2-GA (Production)
- Battle-tested routing logic
- Fallback strategies
- Circuit breaker for unhealthy backends

---

## Success Criteria

1. ✅ GPU tasks never routed to CPU-only pods
2. ✅ <5ms routing overhead (vs current simple proxy)
3. ✅ No 503 errors when capacity exists somewhere
4. ✅ Graceful handling of pod failures
5. ✅ Automatic discovery of new pods

---

## Future Enhancements (v0.3+)

- **Queue-based routing**: If no capacity, queue request instead of 503
- **Predictive routing**: Route based on task duration estimates
- **Affinity routing**: Keep related tasks on same pod
- **Cost-aware routing**: Route to cheaper instances when possible
