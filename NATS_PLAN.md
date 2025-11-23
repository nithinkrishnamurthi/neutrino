# Neutrino: Hybrid Architecture Plan
**Mixed Workload Orchestration with LoadBalancer + NATS**

## Vision

Build a unified orchestration system that handles:
- **Sync tasks** (< 5s): Direct HTTP via LoadBalancer (low latency)
- **Async tasks** (> 5s): NATS JetStream queue (visibility, retries, autoscaling)
- **Specialized workers**: CPU, GPU, memory-intensive pools
- **Unified observability**: Single dashboard for all task types

---

## Current State (v0.1)

```
User → LoadBalancer → Neutrino Pod (HTTP) → Worker Pool
                           ↓
                    ASGI Proxy (optional)
```

**What works:**
- ✅ HTTP server (Axum) routing to Python workers
- ✅ Round-robin load balancing via k8s
- ✅ Sub-100ms latency for sync tasks
- ✅ ASGI fallback for non-Neutrino routes

**What's missing:**
- ❌ No task logging/history
- ❌ No queue visibility
- ❌ No async task support
- ❌ No worker specialization
- ❌ No autoscaling based on workload

---

## Target Architecture (v0.3+)

```
                    ┌─────────────────────────┐
                    │   HTTP Gateway (Axum)   │
                    │  • Route sync → HTTP    │
                    │  • Route async → NATS   │
                    └───────┬─────────────────┘
                            │
              ┌─────────────┴─────────────┐
              │                           │
              ↓ Sync (< 5s)              ↓ Async (> 5s)
      ┌───────────────┐          ┌──────────────────┐
      │ LoadBalancer  │          │  NATS JetStream  │
      └───────┬───────┘          └────────┬─────────┘
              │                           │
         ┌────┴────┐                 ┌────┴─────┐
         ↓         ↓                 ↓          ↓
    ┌────────┐ ┌────────┐      ┌─────────┐ ┌─────────┐
    │Fast Pod│ │Fast Pod│      │Async Pod│ │GPU Pod  │
    └────────┘ └────────┘      └─────────┘ └─────────┘
         │          │                │           │
         └──────────┴────────────────┴───────────┘
                         ↓
                 ┌──────────────┐
                 │  SQLite DB   │
                 │ (Dashboard)  │
                 └──────────────┘
```

---

## Implementation Phases

### **Phase 0: Database Logging (CURRENT PRIORITY)** ⭐

**Goal:** Wire up task execution logging to SQLite database for dashboard visibility.

**Why first:**
- Provides immediate value (task history, debugging)
- Foundation for async task status polling later
- No architectural changes needed
- Can iterate on schema before adding NATS complexity

#### 0.1 Database Schema & Writer

**Files to create:**
- `crates/neutrino-core/src/db_logger.rs` - SQLite writer for task events

**Schema:**
```sql
CREATE TABLE tasks (
    id TEXT PRIMARY KEY,
    function_name TEXT NOT NULL,
    status TEXT NOT NULL,  -- 'pending', 'running', 'completed', 'failed'
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    started_at TIMESTAMP,
    completed_at TIMESTAMP,
    worker_id TEXT,
    pool TEXT,             -- 'fast', 'async', 'gpu'
    execution_mode TEXT,   -- 'sync', 'async'
    args TEXT,             -- JSON serialized
    result TEXT,           -- JSON serialized
    error TEXT,
    duration_ms REAL
);

CREATE INDEX idx_status ON tasks(status);
CREATE INDEX idx_created_at ON tasks(created_at);
CREATE INDEX idx_function_name ON tasks(function_name);
CREATE INDEX idx_pool ON tasks(pool);
```

**Implementation:**
```rust
pub struct DbLogger {
    conn: Arc<Mutex<rusqlite::Connection>>,
}

impl DbLogger {
    pub async fn log_task_start(&self, task_id: &str, function_name: &str, args: &Value) { ... }
    pub async fn log_task_complete(&self, task_id: &str, result: TaskResult, duration_ms: u64) { ... }
    pub async fn log_task_failed(&self, task_id: &str, error: &str) { ... }
}
```

#### 0.2 Integrate into Orchestrator

**Files to modify:**
- `crates/neutrino-core/src/http/mod.rs` - Add DB logging to task execution
- `crates/neutrino-core/src/main.rs` - Initialize DB logger on startup
- `crates/neutrino-core/src/config.rs` - Add database path config

**Flow:**
```rust
async fn execute_task_with_body(...) -> Result<Json<TaskResponse>, AppError> {
    let task_id = uuid::Uuid::new_v4().to_string();

    // 1. Log task start
    db_logger.log_task_start(&task_id, &handler_name, &request.args).await;

    let start = std::time::Instant::now();

    // 2. Execute task (existing logic)
    let result = execute_on_worker(...).await;

    let duration_ms = start.elapsed().as_millis() as u64;

    // 3. Log completion/failure
    match result {
        Ok(res) => db_logger.log_task_complete(&task_id, res, duration_ms).await,
        Err(e) => db_logger.log_task_failed(&task_id, &e.to_string()).await,
    }

    result
}
```

#### 0.3 Shared Database Volume

**Files to modify:**
- `k8s/deployment.yaml` - Add shared volume mount for database
- `k8s/dashboard-deployment.yaml` - Mount same volume

**Volume setup:**
```yaml
# Use existing PVC from dashboard
volumeMounts:
- name: db-storage
  mountPath: /data

volumes:
- name: db-storage
  persistentVolumeClaim:
    claimName: neutrino-db-pvc
```

**Database path:** `/data/neutrino.db` (shared by workers + dashboard)

#### 0.4 Update Dashboard

**Files to modify:**
- `dashboard/app.py` - Update to show real task data, add pool column

**New features:**
- Real-time task list from actual executions
- Filter by status, function name, pool
- Show execution times, error messages
- Search by task ID

**Success criteria:**
- ✅ Tasks appear in dashboard immediately after execution
- ✅ Can see task history across pod restarts
- ✅ Dashboard shows execution times and errors
- ✅ No performance impact on task execution (async writes)

---

### **Phase 1: NATS Infrastructure**

**Goal:** Deploy NATS JetStream alongside existing setup (non-breaking).

#### 1.1 Deploy NATS to k8s

**Files to create:**
- `k8s/nats-deployment.yaml` - NATS StatefulSet with JetStream
- `k8s/nats-service.yaml` - ClusterIP service
- `k8s/nats-configmap.yaml` - JetStream configuration

**Configuration:**
```yaml
jetstream:
  enabled: true
  store_dir: /data/jetstream
  max_memory: 1Gi
  max_file: 10Gi

# Streams
streams:
  - name: NEUTRINO_TASKS
    subjects: ["neutrino.async.*", "neutrino.gpu.*"]
    retention: workqueue
    storage: file
```

#### 1.2 Update CLI

**Files to modify:**
- `python/cli/main.py` - Add NATS deployment to `neutrino up`

**New step in deployment:**
```python
# Apply NATS manifests
run_command("kubectl apply -f k8s/nats-deployment.yaml")
run_command("kubectl apply -f k8s/nats-service.yaml")
```

---

### **Phase 2: Task Configuration System**

**Goal:** Allow users to specify execution mode in Python SDK.

#### 2.1 Python SDK Extensions

**Files to modify:**
- `python/neutrino/app.py` - Add `execution_mode` parameter

**New API:**
```python
@app.task(execution_mode="sync", timeout_ms=1000)
def fast_api(data: dict) -> dict:
    """Low-latency endpoint (< 1s)"""
    return process_quickly(data)

@app.task(execution_mode="async", timeout_secs=300)
def generate_report(dataset: str) -> dict:
    """Long-running task (5min)"""
    return create_report(dataset)

@app.task(execution_mode="gpu", pool="a100", timeout_secs=600)
def train_model(data: str) -> dict:
    """Requires specific GPU"""
    return train_on_gpu(data)
```

**Defaults:**
- `execution_mode="sync"` (backwards compatible)
- `timeout_ms=5000` for sync
- `timeout_secs=300` for async

#### 2.2 OpenAPI Extensions

**Files to modify:**
- `python/neutrino/openapi_generator.py` - Add custom fields

**OpenAPI output:**
```json
{
  "paths": {
    "/api/fast_api": {
      "post": {
        "x-neutrino-mode": "sync",
        "x-neutrino-timeout": 1000
      }
    },
    "/api/generate_report": {
      "post": {
        "x-neutrino-mode": "async",
        "x-neutrino-timeout": 300000
      }
    },
    "/api/train_model": {
      "post": {
        "x-neutrino-mode": "gpu",
        "x-neutrino-pool": "a100",
        "x-neutrino-timeout": 600000
      }
    }
  }
}
```

---

### **Phase 3: Gateway Component**

**Goal:** Smart router that directs sync → HTTP, async → NATS.

#### 3.1 Create Gateway Binary

**Files to create:**
- `crates/neutrino-gateway/` - New crate
  - `Cargo.toml`
  - `src/main.rs` - Entry point
  - `src/router.rs` - Route based on execution mode
  - `src/sync_handler.rs` - HTTP passthrough
  - `src/async_handler.rs` - NATS publisher
  - `src/polling.rs` - Task status API

**Gateway logic:**
```rust
async fn handle_request(
    task_name: String,
    args: Value,
    config: TaskConfig,
) -> Response {
    match config.execution_mode {
        ExecutionMode::Sync => {
            // Pass through to neutrino-fast service
            let response = http_client
                .post(format!("http://neutrino-fast/{}", task_name))
                .json(&args)
                .send()
                .await?;
            Ok(response)
        }
        ExecutionMode::Async | ExecutionMode::Gpu => {
            // Publish to NATS
            let task_id = Uuid::new_v4();
            nats_client.publish(
                format!("neutrino.{}.{}", config.pool, task_name),
                TaskMessage { task_id, args }
            ).await?;

            // Return task ID for polling
            Ok(Json(json!({
                "task_id": task_id,
                "status": "queued",
                "poll_url": format!("/tasks/{}", task_id)
            })))
        }
    }
}
```

#### 3.2 Kubernetes Resources

**Files to create:**
- `k8s/gateway-deployment.yaml` - Gateway pods (2 replicas)
- `k8s/gateway-service.yaml` - LoadBalancer (new user entry point)
- `k8s/gateway-configmap.yaml` - Configuration

**Service mapping:**
```
Before: User → LoadBalancer → neutrino service
After:  User → LoadBalancer → gateway → neutrino-fast service (sync)
                                      → NATS (async)
```

---

### **Phase 4: Worker Pool Specialization**

**Goal:** Multiple worker deployments for different workload types.

#### 4.1 Add Deployment Modes

**Files to modify:**
- `crates/neutrino-core/src/main.rs` - Add `--mode` CLI flag
- `crates/neutrino-core/src/config.rs` - Add pool config

**Modes:**
```rust
enum DeploymentMode {
    Http,  // Start HTTP server (existing)
    Nats,  // Start NATS consumer (new)
}
```

**CLI:**
```bash
# Fast pool (existing behavior)
neutrino serve --mode http --pool fast

# Async pool (new)
neutrino serve --mode nats --pool async --nats-url nats://nats:4222

# GPU pool (new)
neutrino serve --mode nats --pool gpu --nats-url nats://nats:4222
```

#### 4.2 NATS Consumer

**Files to create:**
- `crates/neutrino-core/src/nats_consumer.rs` - NATS subscription logic

**Consumer behavior:**
```rust
pub async fn start_nats_consumer(
    orchestrator: Arc<Orchestrator>,
    nats_url: String,
    pool: String,
) -> Result<()> {
    let client = async_nats::connect(&nats_url).await?;

    let subject = format!("neutrino.{}.*", pool);
    let mut subscriber = client.subscribe(subject).await?;

    info!("Listening on subject: neutrino.{}.*", pool);

    while let Some(msg) = subscriber.next().await {
        let task: TaskMessage = deserialize_nats_message(&msg)?;

        // Log to database
        db_logger.log_task_start(&task.task_id, &task.function_name, &task.args).await;

        // Execute via orchestrator
        let result = orchestrator.execute_task(
            task.function_name,
            task.args
        ).await;

        // Log completion
        db_logger.log_task_complete(&task.task_id, result, duration).await;

        // Acknowledge message
        msg.ack().await?;
    }
}
```

#### 4.3 Multiple Deployments

**Files to create:**
- `k8s/deployment-fast.yaml` - HTTP mode (sync tasks)
- `k8s/deployment-async.yaml` - NATS mode (async tasks)
- `k8s/deployment-gpu.yaml` - NATS mode (GPU tasks)

**Fast pool:**
```yaml
apiVersion: apps/v1
kind: Deployment
metadata:
  name: neutrino-fast
spec:
  replicas: 3
  template:
    spec:
      containers:
      - name: neutrino
        args: ["serve", "--mode", "http", "--pool", "fast"]
```

**Async pool:**
```yaml
apiVersion: apps/v1
kind: Deployment
metadata:
  name: neutrino-async
spec:
  replicas: 2
  template:
    spec:
      containers:
      - name: neutrino
        args: ["serve", "--mode", "nats", "--pool", "async", "--nats-url", "nats://nats:4222"]
```

**GPU pool:**
```yaml
apiVersion: apps/v1
kind: Deployment
metadata:
  name: neutrino-gpu
spec:
  replicas: 0  # Scale to zero when idle
  template:
    spec:
      nodeSelector:
        nvidia.com/gpu: "true"
      containers:
      - name: neutrino
        args: ["serve", "--mode", "nats", "--pool", "gpu", "--nats-url", "nats://nats:4222"]
        resources:
          limits:
            nvidia.com/gpu: 1
```

---

### **Phase 5: Polling API**

**Goal:** Allow clients to check status of async tasks.

#### 5.1 Task Status Endpoints

**Gateway endpoints:**
```
GET /tasks/{task_id}          → Get task status and result
GET /tasks?status=queued      → List tasks by status
DELETE /tasks/{task_id}       → Cancel queued task
```

**Implementation in gateway:**
```rust
async fn get_task_status(task_id: String) -> Json<TaskStatus> {
    // Query database
    let task = db.query_one(
        "SELECT * FROM tasks WHERE id = ?",
        [task_id]
    ).await?;

    Ok(Json(TaskStatus {
        id: task.id,
        status: task.status,
        result: task.result,
        error: task.error,
        created_at: task.created_at,
        completed_at: task.completed_at,
    }))
}
```

#### 5.2 Python Client SDK (Optional)

**Files to create:**
- `python/neutrino/client.py` - HTTP client for Neutrino

```python
class NeutrinoClient:
    def __init__(self, base_url: str):
        self.base_url = base_url

    def call_sync(self, function_name: str, args: dict) -> dict:
        """Call sync task, wait for result."""
        response = requests.post(
            f"{self.base_url}/api/{function_name}",
            json={"args": args}
        )
        return response.json()["result"]

    def call_async(self, function_name: str, args: dict) -> AsyncTask:
        """Call async task, return task handle."""
        response = requests.post(
            f"{self.base_url}/api/{function_name}",
            json={"args": args}
        )
        return AsyncTask(self, response.json()["task_id"])

class AsyncTask:
    def wait(self, timeout: int = 300) -> dict:
        """Poll until task completes."""
        start = time.time()
        while time.time() - start < timeout:
            status = self.client.get_task_status(self.task_id)
            if status["status"] == "completed":
                return status["result"]
            elif status["status"] == "failed":
                raise Exception(status["error"])
            time.sleep(1)
        raise TimeoutError()
```

---

### **Phase 6: Autoscaling**

**Goal:** Scale worker pools based on workload.

#### 6.1 Fast Pool (CPU-based)

**Files to create:**
- `k8s/hpa-fast.yaml`

```yaml
apiVersion: autoscaling/v2
kind: HorizontalPodAutoscaler
metadata:
  name: neutrino-fast-hpa
spec:
  scaleTargetRef:
    name: neutrino-fast
  minReplicas: 2
  maxReplicas: 20
  metrics:
  - type: Resource
    resource:
      name: cpu
      target:
        type: Utilization
        averageUtilization: 70
```

#### 6.2 Async Pool (Queue-based)

**Files to create:**
- `k8s/hpa-async.yaml`
- `k8s/nats-metrics-exporter.yaml` - Prometheus exporter for NATS

```yaml
apiVersion: autoscaling/v2
kind: HorizontalPodAutoscaler
metadata:
  name: neutrino-async-hpa
spec:
  scaleTargetRef:
    name: neutrino-async
  minReplicas: 1
  maxReplicas: 50
  metrics:
  - type: External
    external:
      metric:
        name: nats_consumer_num_pending
        selector:
          matchLabels:
            subject: neutrino.async.*
      target:
        type: AverageValue
        averageValue: "10"
```

#### 6.3 GPU Pool (Cost-conscious)

**Files to create:**
- `k8s/hpa-gpu.yaml`

```yaml
# Scale conservatively due to GPU cost
minReplicas: 0   # Scale to zero when idle
maxReplicas: 5   # Limit max GPU instances
scaleDown:
  stabilizationWindowSeconds: 300  # Wait 5min before scaling down
```

---

## Migration Path

### Step 1: Add Database Logging ⭐ (START HERE)
- ✅ No breaking changes
- ✅ Immediate value (task visibility)
- ✅ Foundation for async tasks

### Step 2: Deploy NATS (alongside existing)
- NATS deployed but not used yet
- Existing HTTP flow unchanged

### Step 3: Deploy Gateway (testing)
- Gateway routes to existing service
- Can test before switching traffic

### Step 4: Add Async Pool
- Deploy async workers
- Enable async tasks for specific functions
- Sync tasks still work as before

### Step 5: Add GPU Pool (when needed)
- Deploy GPU workers
- Route GPU tasks via NATS

### Step 6: Optimize & Scale
- Add autoscaling
- Tune performance
- Monitor queue depths

---

## Key Dependencies

### Rust
```toml
# neutrino-core
async-nats = "0.33"
rusqlite = { version = "0.31", features = ["bundled"] }
tokio = { version = "1", features = ["full"] }

# neutrino-gateway
async-nats = "0.33"
axum = "0.7"
reqwest = { version = "0.12", features = ["json"] }
```

### Kubernetes
- NATS JetStream: `nats:2.10-alpine`
- NATS Prometheus Exporter: `natsio/prometheus-nats-exporter:0.14.0`

### Python
```toml
# Optional client SDK
requests = "2.31.0"
```

---

## Success Metrics

### Phase 0 (Database Logging)
- ✅ All tasks visible in dashboard
- ✅ Task history persists across pod restarts
- ✅ < 1ms overhead for DB writes
- ✅ Dashboard shows real-time task status

### Phase 3 (Gateway + NATS)
- ✅ Sync tasks maintain < 100ms p95 latency
- ✅ Async tasks return task_id < 10ms
- ✅ Can poll task status via API
- ✅ Queue depth visible in metrics

### Phase 4 (Worker Pools)
- ✅ Can deploy GPU workers separately
- ✅ Workers pull from correct NATS subjects
- ✅ All task types logged to unified DB

### Phase 6 (Autoscaling)
- ✅ Async pool scales based on queue depth
- ✅ GPU pool scales to zero when idle
- ✅ Fast pool scales on CPU utilization

---

## Timeline Estimate

| Phase | Description | Effort |
|-------|-------------|--------|
| 0 | Database logging | 4 hours |
| 1 | NATS infrastructure | 3 hours |
| 2 | Task configuration | 2 hours |
| 3 | Gateway component | 6 hours |
| 4 | Worker pools | 4 hours |
| 5 | Polling API | 2 hours |
| 6 | Autoscaling | 2 hours |
| **Total** | | **23 hours** |

**Incremental delivery:**
- Phase 0: Immediate value (database visibility)
- Phases 1-3: Enable async tasks
- Phases 4-6: Production hardening

---

## Open Questions

1. **Database approach:**
   - Option A: Shared SQLite via PVC (simple, single-node limit)
   - Option B: Workers publish events to NATS, dashboard subscribes (scalable)
   - **Decision:** Start with Option A, migrate to B if needed

2. **NATS persistence:**
   - How long to retain completed tasks in NATS?
   - **Decision:** Use WorkQueue retention (delete after ack), rely on SQLite for history

3. **Task cancellation:**
   - How to cancel in-progress tasks?
   - **Decision:** Phase 2+ feature, implement via worker heartbeat checks

4. **Result storage:**
   - Store large results in DB or object storage?
   - **Decision:** Start with DB (< 1MB results), add S3 support later if needed

---

## References

- NATS JetStream docs: https://docs.nats.io/nats-concepts/jetstream
- Kubernetes HPA: https://kubernetes.io/docs/tasks/run-application/horizontal-pod-autoscale/
- Ray architecture: https://docs.ray.io/en/latest/ray-core/architecture.html
- Celery design: https://docs.celeryq.dev/en/stable/internals/guide.html
