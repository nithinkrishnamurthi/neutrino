Project Context for AI Assistants
This document provides context about Neutrino for AI coding assistants like Claude, to help with development, debugging, and architectural discussions.

Project Overview
Neutrino is a high-performance distributed orchestration and model serving framework for Python, with a Rust core. Think "FastAPI meets Ray" - it provides FastAPI-like ergonomics with Ray-level performance, unified with native model serving capabilities.
Core Value Proposition

Unified abstraction: Orchestration + model serving in one framework
Performance: Sub-100ms task dispatch (vs. Airflow's 400ms+)
Developer experience: FastAPI-style decorators and pythonic API
Production-ready: Built-in autoscaling, observability, and lifecycle management

Market Position
Neutrino fills the gap between existing tools:

Airflow/Prefect: Great DX, terrible performance (400ms+ latency)
Ray: Great performance, complex API, steep learning curve
KServe/Seldon: Great for models, no orchestration primitives
Neutrino: FastAPI-like simplicity + Ray-like performance + unified model serving


Architecture
Deployment Model: Orchestrator-per-Pod
Critical decision: We use orchestrator-per-pod architecture, NOT centralized orchestrator.
┌─────────────────────────────────┐
│           Pod 1                 │
│  ┌──────────────────────┐       │
│  │ Rust Orchestrator    │       │
│  └────────┬─────────────┘       │
│           │ Unix sockets        │
│      ┌────┼────┬────┐           │
│      ▼    ▼    ▼    ▼           │
│    [Py] [Py] [Py] [Py]          │
│    Workers (8-16)                │
└─────────────────────────────────┘

┌─────────────────────────────────┐
│           Pod 2                 │
│  ┌──────────────────────┐       │
│  │ Rust Orchestrator    │       │
│  └────────┬─────────────┘       │
│           │ Unix sockets        │
│      ┌────┼────┬────┐           │
│      ▼    ▼    ▼    ▼           │
│    [Py] [Py] [Py] [Py]          │
│    Workers (8-16)                │
└─────────────────────────────────┘

         Load Balancer
              ↓
        (distributes requests)
Why this matters:

✅ Each pod is self-contained (orchestrator + workers)
✅ Unix domain sockets for fast communication (within pod)
✅ Standard k8s deployment patterns (StatefulSet/Deployment)
✅ Horizontal scaling via pod replication
✅ No distributed coordination needed (simpler!)
✅ Works locally same as in production

What this means for development:

Communication within pod: Unix domain sockets
Communication across pods: HTTP/gRPC (for model serving)
Each orchestrator manages its own worker pool
No shared state between orchestrators (stateless pods)

Three-Tier Architecture
┌─────────────────────────────────────────────────────────┐
│                    Tier 1: Gateway                      │
│                  (k8s LoadBalancer/Ingress)             │
└────────────────────────┬────────────────────────────────┘
                         │
          ┌──────────────┴──────────────┐
          │                             │
          ▼                             ▼
┌─────────────────────┐       ┌─────────────────────┐
│ Tier 2: Task Pods   │       │ Tier 3: Model Pods  │
│ (Orchestrator+Workers)│      │ (Model Servers)     │
│                     │       │                     │
│ ┌─────────────┐     │       │ ┌─────────────┐     │
│ │Orchestrator │     │       │ │Model Server │     │
│ └──────┬──────┘     │       │ └──────┬──────┘     │
│   ┌────┴───┐        │       │   ┌────┴───┐        │
│   │Workers │        │       │   │ModelA  │        │
│   │(generic)│       │       │   │Replicas│        │
│   └────────┘        │       │   └────────┘        │
└─────────────────────┘       └─────────────────────┘
    Scale by:                     Scale by:
    - Queue depth                 - Request rate
    - CPU usage                   - Per-model metrics
Tier 2 - Task Orchestration Pods:

1 Rust orchestrator per pod
8-16 Python workers per pod
Handle @app.task() functions
Scale based on task queue depth

Tier 3 - Model Serving Pods:

One deployment PER model
Lightweight model server
Independent autoscaling per model
Handle @app.model() classes

Why separate tiers?

Tasks and models have different scaling characteristics
Models need N identical replicas (homogeneous)
Tasks need heterogeneous workers
Can't efficiently scale mixed workload in single pod


Core Components
1. Rust Orchestration Layer (/rust)
Purpose: Performance-critical scheduling and lifecycle management
Key modules:

orchestrator.rs - Main event loop
worker/spawner.rs - Spawn Python workers
worker/manager.rs - Worker pool management
queue.rs - Task queue implementation
protocol/message.rs - Message types for IPC
metrics.rs - Prometheus metrics

Communication:

Within pod: Unix domain sockets
Across pods: HTTP/gRPC (for model calls)

Responsibilities:

Spawn and monitor Python workers
Task queue management
Worker lifecycle (spawn, monitor, recycle)
Memory monitoring via /proc/<pid>/status
Scheduling (hybrid push-on-idle, pull-on-busy)
Metrics collection

2. Python SDK (/python/neutrino)
Purpose: User-facing API and worker runtime
Key modules:

app.py - Main App() class with decorators
task.py - Task definition and execution
model.py - Model registration and serving
worker/main.py - Worker process entry point
worker/protocol.py - Message handling
bridge.py - Rust ↔ Python FFI via PyO3

3. Model Serving Layer (/neutrino/serving)
Purpose: ML model deployment and autoscaling
Architecture:

Separate k8s Deployment per model
Lightweight HTTP/gRPC server per model pod
Independent HorizontalPodAutoscaler per model
Task pods call model pods via HTTP


Key Design Decisions
1. Why Rust + Python?
Rust for orchestration:

Sub-100ms latency requirement
Memory-safe process management
Zero-cost abstractions for scheduling
Native async/await with Tokio

Python for user code:

ML practitioners' primary language
Access to entire ML ecosystem
No learning curve for end users
Dynamic execution for flexibility

2. Communication Protocol
Within Pod (Orchestrator ↔ Workers):

Unix domain sockets
Wire format: [4 bytes: length][N bytes: msgpack payload]
Message types:

rust  enum Message {
      TaskAssignment { task_id, function_name, args },
      TaskResult { task_id, success, result },
      WorkerReady { worker_id, capabilities },
      Heartbeat { worker_id, stats },
      Shutdown { graceful: bool },
  }
Across Pods (Task Pod ↔ Model Pod):

HTTP/gRPC
msgpack serialization
Service discovery via k8s Services

Why msgpack?

Fast serialization
Compact binary format
Good Python type support
Better than JSON for binary data

3. Task Scheduling Strategy
Hybrid: Push-on-Idle, Pull-on-Busy
rustenum WorkerState {
    Idle,     // ← Orchestrator can PUSH to this worker
    Busy,     // ← Worker will PULL next task when done
    Recycling,
}
```

**How it works:**
1. Worker starts → Sends "Ready" → Marked as Idle
2. Task arrives, worker is Idle → Orchestrator **pushes** immediately (no round-trip!)
3. Worker executes task → Becomes Busy
4. Worker finishes → Sends result + **pulls** next task in same message
5. If queue empty → Worker back to Idle, waits for push

**Benefits:**
- Low latency for first task (push when idle)
- Self-balancing for subsequent tasks (pull when busy)
- No wasted round-trips
- Natural backpressure

**Alternatives considered:**
- Pure pull: Extra round-trip kills <100ms target
- Pure push: Complex load balancing, risk of overload

### 4. Memory Management Strategy

**Problem**: Python's reference counting breaks copy-on-write, causing memory bloat in forked processes.

**Solution**: Periodic worker recycling

Workers recycled based on:
- **Request count**: After N tasks (e.g., 1000 tasks)
- **Memory usage**: When RSS exceeds threshold (e.g., 4GB)
- **Time-based**: Maximum worker lifetime (e.g., 1 hour)

**Implementation:**
```
Master Process (Rust) forks → Worker 1
  • Inherits loaded libraries via COW
  • Executes tasks
  • Tracks: task_count, memory_rss, start_time
  • Exits when threshold hit
  ↓
Rust spawns replacement worker
Why this works:

Simple and proven (Gunicorn, uWSGI use this)
Predictable performance
Prevents memory leaks
Libraries stay "hot" via pre-fork pattern

Future optimizations (v0.3+):

Python 3.13 free-threading support
Shared memory primitives for read-only data
CRIU-like snapshotting (research)

5. Model Serving Integration
Why separate deployments?
Models scale differently than tasks:

Models: N identical replicas of SAME model
Tasks: Heterogeneous workers with different code

Architecture:
python# User defines model
@app.model(name="sentiment", min_replicas=1, max_replicas=10)
class SentimentAnalyzer:
    def load(self):
        self.model = pipeline("sentiment-analysis")
    
    def predict(self, text: str):
        return self.model(text)

# Used in task
@app.task()
async def analyze(text: str):
    # Makes HTTP call to model pod
    return await app.models.sentiment.predict(text)
Deployment creates:
yaml# One deployment PER model
Deployment: neutrino-model-sentiment
Service: neutrino-model-sentiment
HorizontalPodAutoscaler: neutrino-model-sentiment-hpa
```

**Task → Model communication:**
1. Task pod makes HTTP request to `http://neutrino-model-sentiment:8080`
2. k8s Service load balances to model pod
3. Model pod runs inference
4. Returns result
5. Task continues

---

## Development Phases

### Phase 1: Single-Machine MVP (v0.1) ← CURRENT FOCUS

**First ticket:** Spawn Python worker and establish Unix socket communication

**Components to build:**
1. ✅ Worker spawning (Rust spawns Python process)
2. ✅ Unix socket communication
3. ✅ Message protocol (msgpack serialization)
4. [ ] Task queue (VecDeque in Rust)
5. [ ] Task execution (send task to worker, get result)
6. [ ] Worker pool (spawn N workers)
7. [ ] Worker recycling (memory monitoring)
8. [ ] Python SDK (`@app.task()` decorator)
9. [ ] Basic metrics (Prometheus)

**Not included in v0.1:**
- Model serving (comes in v0.2)
- Autoscaling
- Retries
- Persistence
- Multi-node

### Phase 2: Production Hardening (v0.2)

**Add:**
- Model serving tier
- Retry logic with exponential backoff
- State persistence (RocksDB or SQLite)
- Better observability (OpenTelemetry)
- Autoscaling based on queue depth
- Graceful shutdown
- Configuration hot reload
- Kubernetes deployment

### Phase 3: Scale Out (v0.3)

**Add:**
- Work stealing between pods (if needed)
- Advanced GPU scheduling
- Python 3.13 free-threading support
- Distributed tracing
- Multi-region support

---

## Code Organization
```
neutrino/
├── rust/                    # Rust orchestration core
│   ├── src/
│   │   ├── main.rs          # Entry point
│   │   ├── orchestrator.rs  # Main event loop
│   │   ├── worker/
│   │   │   ├── mod.rs
│   │   │   ├── spawner.rs   # ← FIRST TICKET: Spawn workers
│   │   │   └── manager.rs   # Worker pool management
│   │   ├── queue.rs         # Task queue
│   │   ├── protocol/
│   │   │   ├── mod.rs
│   │   │   └── message.rs   # ← FIRST TICKET: Message types
│   │   └── metrics.rs       # Prometheus metrics
│   └── Cargo.toml
├── python/neutrino/         # Python SDK
│   ├── __init__.py
│   ├── app.py               # Main App class
│   ├── task.py              # Task decorators
│   ├── model.py             # Model decorators
│   ├── worker/
│   │   ├── __init__.py
│   │   ├── main.py          # ← FIRST TICKET: Worker entry point
│   │   └── protocol.py      # ← FIRST TICKET: Message handling
│   └── bridge.py            # PyO3 bindings (later)
├── examples/                # Usage examples
├── docs/                    # Documentation
├── benchmarks/              # Performance tests
└── tests/                   # Test suite

Current Development Focus
First Ticket: Worker Spawning and Socket Communication
Goal: Get Rust orchestrator to spawn a Python worker process and establish bidirectional communication via Unix domain socket.
Steps:

Rust spawns Python process
Create Unix domain socket
Python worker connects to socket
Worker sends "Ready" message (msgpack)
Orchestrator receives and parses message
Bidirectional communication established

Files to create:

rust/src/worker/spawner.rs
rust/src/protocol/message.rs
python/neutrino/worker/main.py
python/neutrino/worker/protocol.py

Acceptance criteria:
bash$ cargo run
[INFO] Starting orchestrator
[INFO] Spawning worker worker-001
[INFO] Waiting for worker to connect...
[INFO] Worker worker-001 connected (pid=12345)
[INFO] Worker worker-001 ready!
[INFO] Orchestrator ready with 1 worker
Why this first?

Proves Rust ↔ Python IPC (biggest risk)
Forces protocol decisions early
Tangible progress (can demo)
Unblocks all future work


Important Constraints
Communication Boundaries
Within Pod:

✅ Unix domain sockets (fast, <1ms latency)
✅ Shared memory possible (future optimization)

Across Pods:

❌ Unix sockets DON'T work across network
✅ Must use HTTP/gRPC
✅ k8s Service for discovery

This is why we separate task and model tiers - models need cross-pod communication anyway, so we isolate that complexity.
Kubernetes Deployment
Each Neutrino pod contains:

1 Rust orchestrator process (parent)
8-16 Python worker processes (children)
All communicate via Unix sockets
One container image with both Rust and Python

Container startup:
dockerfileFROM python:3.11

# Install Rust binary (compiled separately)
COPY --from=builder /app/neutrino /usr/local/bin/

# Copy Python code
COPY python/ /usr/local/lib/python3.11/site-packages/neutrino/

# Entry point
ENTRYPOINT ["neutrino", "serve"]
Scaling:

HorizontalPodAutoscaler scales pods (not individual workers)
Each pod is identical and stateless
Load balancer distributes across pods


Performance Targets
MetricTargetWhyTask dispatch latency<100ms p95Competitive with Ray, way better than AirflowThroughput>10k tasks/sec (16 cores)Reasonable for most workloadsMemory overhead<20% vs single processCOW sharing should keep this lowWorker startup<50msPre-forked, so should be instant

Testing Strategy
Unit Tests
Rust side:
bashcargo test
Python side:
bashpytest tests/unit/
Integration Tests
End-to-end scenarios:

Spawn worker and send task
Worker recycling on memory limit
Task retries on failure
Model serving integration

Performance Tests
Benchmarks:

Task dispatch latency (measure in Rust)
Throughput (tasks/second)
Memory efficiency (compare to baseline)

Tools:

Criterion for Rust benchmarks
py-spy for Python profiling
perf for system-level profiling


Common Development Tasks
Running Locally
bash# Build Rust
cd rust/
cargo build

# Run orchestrator
cargo run

# Run tests
cargo test
pytest
Debugging
Rust side:
bash# Enable debug logging
RUST_LOG=debug cargo run

# Run with debugger
rust-lldb target/debug/neutrino
Python side:
bash# Enable debug logging
NEUTRINO_LOG=debug cargo run

# Attach to worker
neutrino dev --debug-worker

# Or use pdb
import pdb; pdb.set_trace()
Profiling
Rust:
bashcargo build --release
perf record ./target/release/neutrino
perf report
Python:
bashpy-spy record -o profile.svg -- python worker.py

Key Technologies
Rust Dependencies
toml[dependencies]
tokio = { version = "1", features = ["full"] }  # Async runtime
rmp-serde = "1.1"                               # msgpack serialization
serde = { version = "1.0", features = ["derive"] }
tracing = "0.1"                                 # Logging
tracing-subscriber = "0.3"
```

### Python Dependencies
```
msgpack>=1.0.0       # Serialization
aiohttp>=3.8.0       # Async HTTP (for model calls)
pydantic>=2.0.0      # Validation

Troubleshooting Guide
Workers not starting
Check:

Rust orchestrator running: ps aux | grep neutrino
Python in PATH: which python3
Worker script exists: ls python/neutrino/worker/main.py
Socket permissions: ls -l /tmp/neutrino-*.sock

Socket connection failures
Common issues:

Old socket file exists: rm /tmp/neutrino-*.sock
Permission denied: Check file permissions
Connection timeout: Worker taking too long to start

High memory usage
Check:

Worker recycling config: --max-requests and --max-memory
Memory leaks in user code: Profile with memory_profiler
Shared memory usage: cat /proc/<pid>/smaps | grep Shared


Questions for AI Assistants
When uncertain, please ask:

Deployment context: Is this for single-machine (v0.1) or k8s (v0.2+)?
Communication: Is this within-pod (Unix sockets) or across-pod (HTTP)?
Performance: What are the latency/throughput requirements?
Scope: Is this for v0.1 MVP or future version?
Breaking changes: Will this affect the public API?


Quick Reference
Architecture Summary

Deployment: Orchestrator-per-pod (not centralized)
Communication: Unix sockets within pod, HTTP across pods
Scheduling: Hybrid push-on-idle, pull-on-busy
Memory: Periodic worker recycling (not state reversion)
Models: Separate deployment tier, independent scaling

Key Files

rust/src/worker/spawner.rs - Worker spawning ← START HERE
python/neutrino/worker/main.py - Worker entry point ← START HERE
rust/src/protocol/message.rs - Message types
python/neutrino/app.py - User-facing App class

Next Steps

✅ First ticket: Worker spawning + socket communication
 Task queue implementation
 Task execution (send/receive)
 Worker pool management
 Python SDK (@app.task())