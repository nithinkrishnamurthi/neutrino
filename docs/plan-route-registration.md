# Route Registration & Serve CLI Implementation Plan

## Goal

Enable end-to-end request flow:

```bash
$ neutrino serve myapp:app --workers 8 --port 8000
```

Where `myapp.py`:
```python
from neutrino import App

app = App()

@app.route("/add")
def add(x: int, y: int):
    return {"result": x + y}

@app.route("/process", methods=["POST"])
def process(data: dict):
    return {"processed": data}
```

Then:
```bash
$ curl -X POST http://localhost:8000/add -d '{"x": 2, "y": 3}'
{"result": 5}
```

---

## Architecture

### Startup Flow

```
neutrino serve myapp:app --workers 8 --port 8000
                    │
                    ▼
        ┌─────────────────────┐
        │   Rust Orchestrator │
        │      (main.rs)      │
        └──────────┬──────────┘
                   │
        ┌──────────▼──────────┐
        │  Spawn Discovery    │
        │      Worker         │
        └──────────┬──────────┘
                   │ Unix Socket
                   ▼
        ┌─────────────────────┐
        │   Python Worker     │
        │ import myapp:app    │
        │ app.list_routes()   │
        └──────────┬──────────┘
                   │
                   ▼
        ┌─────────────────────┐
        │  RouteRegistry msg  │
        │  [("/add", ["GET"]),│
        │   ("/process",      │
        │    ["POST"])]       │
        └──────────┬──────────┘
                   │
                   ▼
        ┌─────────────────────┐
        │  Rust: Register     │
        │  HTTP routes        │
        └──────────┬──────────┘
                   │
                   ▼
        ┌─────────────────────┐
        │  Spawn Worker Pool  │
        │  (8 workers)        │
        └──────────┬──────────┘
                   │
                   ▼
        ┌─────────────────────┐
        │  Start HTTP Server  │
        │  (port 8000)        │
        └─────────────────────┘
                   │
                   ▼
              Ready to serve
```

### Request Flow

```
HTTP Request: POST /add {"x": 2, "y": 3}
                    │
                    ▼
        ┌─────────────────────┐
        │  Rust HTTP Server   │
        │  (axum/hyper)       │
        └──────────┬──────────┘
                   │
                   ▼
        ┌─────────────────────┐
        │  Route Lookup       │
        │  /add → exists      │
        │  POST → allowed?    │
        └──────────┬──────────┘
                   │ If valid
                   ▼
        ┌─────────────────────┐
        │  Select Worker      │
        │  (idle worker from  │
        │   pool)             │
        └──────────┬──────────┘
                   │
                   ▼
        ┌─────────────────────┐
        │  TaskAssignment msg │
        │  task_id: uuid      │
        │  path: "/add"       │
        │  method: "POST"     │
        │  body: {"x":2,"y":3}│
        └──────────┬──────────┘
                   │ Unix Socket
                   ▼
        ┌─────────────────────┐
        │  Python Worker      │
        │  route = app.get_   │
        │    route("/add")    │
        │  result = route(**  │
        │    body)            │
        └──────────┬──────────┘
                   │
                   ▼
        ┌─────────────────────┐
        │  TaskResult msg     │
        │  task_id: uuid      │
        │  success: true      │
        │  result: {"result": │
        │           5}        │
        └──────────┬──────────┘
                   │ Unix Socket
                   ▼
        ┌─────────────────────┐
        │  Rust: Serialize    │
        │  HTTP Response      │
        └──────────┬──────────┘
                   │
                   ▼
        HTTP Response: 200 OK
        {"result": 5}
```

---

## Implementation Phases

### Phase 1: Protocol Extensions

**Goal**: Add message types for route discovery and task execution.

#### Rust: `crates/neutrino-core/src/protocol/mod.rs`

```rust
#[derive(Debug, Serialize, Deserialize)]
pub struct RouteInfo {
    pub path: String,
    pub methods: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub enum Message {
    // Existing
    WorkerReady { worker_id: String, pid: u32 },
    TaskResult { task_id: String, success: bool, result: Vec<u8> },
    Shutdown { graceful: bool },
    Heartbeat { worker_id: String },

    // New: Discovery
    DiscoverRoutes,
    RouteRegistry { routes: Vec<RouteInfo> },

    // Updated: Task execution
    TaskAssignment {
        task_id: String,
        path: String,           // Route path
        method: String,         // HTTP method
        headers: Vec<(String, String)>,
        body: Vec<u8>,          // Request body (JSON)
    },
}
```

#### Python: `python/neutrino/internal/worker/protocol.py`

```python
def send_route_registry(self, routes: list[tuple[str, list[str]]]) -> None:
    """Send RouteRegistry message."""
    route_infos = [
        {"path": path, "methods": methods}
        for path, methods in routes
    ]
    self.send({"RouteRegistry": {"routes": route_infos}})
```

**Files to modify**:
- `crates/neutrino-core/src/protocol/mod.rs`
- `python/neutrino/internal/worker/protocol.py`

---

### Phase 2: Worker App Loading

**Goal**: Worker can import user's app module and report registered routes.

#### Python: `python/neutrino/internal/worker/main.py`

```python
#!/usr/bin/env python3
"""
Usage: python -m neutrino.internal.worker.main <socket_path> <worker_id> <app_path>
       app_path format: "module:attribute" (e.g., "myapp:app")
"""

import importlib
import sys

def load_app(app_path: str):
    """Load app from 'module:attribute' format."""
    module_name, attr_name = app_path.split(":")
    module = importlib.import_module(module_name)
    return getattr(module, attr_name)

def main():
    socket_path = sys.argv[1]
    worker_id = sys.argv[2]
    app_path = sys.argv[3]

    # Load user's app
    app = load_app(app_path)

    # Connect to orchestrator
    sock = connect(socket_path)
    protocol = ProtocolHandler(sock)

    # Send ready with route registry
    routes = [
        (path, app.get_route(path).methods)
        for path in app.list_routes()
    ]
    protocol.send_ready(worker_id, os.getpid())
    protocol.send_route_registry(routes)

    # Main loop: execute tasks
    while True:
        msg = protocol.recv()
        if "TaskAssignment" in msg:
            result = execute_route(app, msg["TaskAssignment"])
            protocol.send_task_result(...)
        elif "Shutdown" in msg:
            break
```

#### Route Execution Logic

```python
def execute_route(app, task: dict) -> dict:
    """Execute a route and return result."""
    path = task["path"]
    method = task["method"]
    body = json.loads(task["body"]) if task["body"] else {}

    route = app.get_route(path)

    # Validate method
    if method not in route.methods:
        return {"success": False, "error": "Method not allowed"}

    # Execute handler
    try:
        if isinstance(body, dict):
            result = route(**body)
        else:
            result = route(body)
        return {"success": True, "result": result}
    except Exception as e:
        return {"success": False, "error": str(e)}
```

**Files to modify**:
- `python/neutrino/internal/worker/main.py`
- `python/neutrino/internal/worker/executor.py` (new)

---

### Phase 3: HTTP Server in Rust

**Goal**: Rust orchestrator accepts HTTP requests and forwards to workers.

#### Dependencies: `crates/neutrino-core/Cargo.toml`

```toml
[dependencies]
axum = "0.7"
tower = "0.4"
hyper = { version = "1.0", features = ["full"] }
http-body-util = "0.1"
```

#### New Module: `crates/neutrino-core/src/http/mod.rs`

```rust
use axum::{
    Router,
    routing::{get, post, put, delete},
    extract::{State, Path, Json},
    response::Json as JsonResponse,
    http::StatusCode,
};
use std::sync::Arc;
use tokio::sync::Mutex;

pub struct HttpServer {
    router: Router,
    orchestrator: Arc<Mutex<Orchestrator>>,
}

impl HttpServer {
    pub fn new(orchestrator: Arc<Mutex<Orchestrator>>) -> Self {
        let router = Router::new()
            .fallback(handle_request)
            .with_state(orchestrator.clone());

        Self { router, orchestrator }
    }

    pub fn register_routes(&mut self, routes: Vec<RouteInfo>) {
        // Build router from discovered routes
        for route in routes {
            // Register path with appropriate methods
        }
    }

    pub async fn serve(self, addr: &str) -> Result<(), Box<dyn Error>> {
        let listener = TcpListener::bind(addr).await?;
        axum::serve(listener, self.router).await?;
        Ok(())
    }
}

async fn handle_request(
    State(orchestrator): State<Arc<Mutex<Orchestrator>>>,
    method: Method,
    path: Path<String>,
    body: Bytes,
) -> Result<JsonResponse<Value>, StatusCode> {
    let mut orch = orchestrator.lock().await;

    // Find available worker
    let worker = orch.get_idle_worker()?;

    // Send task assignment
    let task_id = Uuid::new_v4().to_string();
    worker.send(&Message::TaskAssignment {
        task_id: task_id.clone(),
        path: path.0,
        method: method.to_string(),
        headers: vec![],
        body: body.to_vec(),
    }).await?;

    // Wait for result
    let result = worker.recv().await?;

    match result {
        Message::TaskResult { success, result, .. } => {
            if success {
                Ok(JsonResponse(serde_json::from_slice(&result)?))
            } else {
                Err(StatusCode::INTERNAL_SERVER_ERROR)
            }
        }
        _ => Err(StatusCode::INTERNAL_SERVER_ERROR)
    }
}
```

**Files to create**:
- `crates/neutrino-core/src/http/mod.rs`
- `crates/neutrino-core/src/http/server.rs`
- `crates/neutrino-core/src/http/router.rs`

**Files to modify**:
- `crates/neutrino-core/Cargo.toml`
- `crates/neutrino-core/src/lib.rs`

---

### Phase 4: CLI Entry Point

**Goal**: `neutrino serve` command with argument parsing.

#### Dependencies: `crates/neutrino-core/Cargo.toml`

```toml
[dependencies]
clap = { version = "4.0", features = ["derive"] }
```

#### CLI: `crates/neutrino-core/src/main.rs`

```rust
use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "neutrino")]
#[command(about = "High-performance distributed orchestration framework")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Start the orchestrator and serve the application
    Serve {
        /// Application path in format "module:attribute"
        #[arg(value_name = "APP")]
        app_path: String,

        /// Number of worker processes
        #[arg(short, long, default_value = "4")]
        workers: usize,

        /// Port to listen on
        #[arg(short, long, default_value = "8000")]
        port: u16,

        /// Host to bind to
        #[arg(long, default_value = "127.0.0.1")]
        host: String,
    },
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Serve { app_path, workers, port, host } => {
            serve(app_path, workers, port, host).await?;
        }
    }

    Ok(())
}

async fn serve(
    app_path: String,
    num_workers: usize,
    port: u16,
    host: String,
) -> Result<(), Box<dyn Error>> {
    info!("Starting Neutrino orchestrator");
    info!("App: {}", app_path);
    info!("Workers: {}", num_workers);
    info!("Listening on {}:{}", host, port);

    // 1. Spawn discovery worker
    let discovery_worker = WorkerHandle::spawn_discovery(&app_path).await?;

    // 2. Get route registry
    let routes = discovery_worker.get_routes().await?;
    info!("Discovered {} routes", routes.len());
    for route in &routes {
        info!("  {} {:?}", route.path, route.methods);
    }

    // 3. Shutdown discovery worker
    discovery_worker.shutdown().await?;

    // 4. Create orchestrator with worker pool
    let orchestrator = Orchestrator::new(&app_path, num_workers).await?;

    // 5. Create HTTP server with routes
    let mut http_server = HttpServer::new(orchestrator);
    http_server.register_routes(routes);

    // 6. Start serving
    let addr = format!("{}:{}", host, port);
    info!("Server ready at http://{}", addr);
    http_server.serve(&addr).await?;

    Ok(())
}
```

**Files to modify**:
- `crates/neutrino-core/src/main.rs`
- `crates/neutrino-core/Cargo.toml`

---

### Phase 5: Orchestrator & Worker Pool

**Goal**: Manage multiple workers and route requests to them.

#### Orchestrator: `crates/neutrino-core/src/orchestrator/mod.rs`

```rust
pub struct Orchestrator {
    workers: Vec<WorkerHandle>,
    app_path: String,
}

impl Orchestrator {
    pub async fn new(app_path: &str, num_workers: usize) -> Result<Self, Error> {
        let mut workers = Vec::with_capacity(num_workers);

        for i in 0..num_workers {
            let worker_id = format!("worker-{}", i);
            let handle = WorkerHandle::spawn(worker_id, app_path).await?;
            workers.push(handle);
        }

        Ok(Self {
            workers,
            app_path: app_path.to_string(),
        })
    }

    pub fn get_idle_worker(&mut self) -> Option<&mut WorkerHandle> {
        self.workers.iter_mut().find(|w| w.is_idle())
    }
}
```

#### Worker Handle Updates: `crates/neutrino-core/src/worker/mod.rs`

```rust
impl WorkerHandle {
    pub async fn spawn(worker_id: String, app_path: &str) -> Result<Self, Error> {
        // ... existing socket setup ...

        let process = Command::new("python3")
            .arg("-m")
            .arg("neutrino.internal.worker.main")
            .arg(&socket_path)
            .arg(&worker_id)
            .arg(app_path)  // NEW: Pass app path
            .spawn()?;

        // ... rest of spawn logic ...
    }

    pub fn is_idle(&self) -> bool {
        matches!(self.worker.state, WorkerState::Idle)
    }
}
```

**Files to modify**:
- `crates/neutrino-core/src/orchestrator/mod.rs`
- `crates/neutrino-core/src/worker/mod.rs`

---

## Complete File Change Summary

### New Files

```
crates/neutrino-core/src/http/mod.rs
crates/neutrino-core/src/http/server.rs
python/neutrino/internal/worker/executor.py
```

### Modified Files

```
crates/neutrino-core/Cargo.toml              # Add axum, clap dependencies
crates/neutrino-core/src/lib.rs              # Export http module
crates/neutrino-core/src/main.rs             # CLI with serve command
crates/neutrino-core/src/protocol/mod.rs     # New message types
crates/neutrino-core/src/worker/mod.rs       # Pass app_path to worker
crates/neutrino-core/src/orchestrator/mod.rs # Worker pool management

python/neutrino/internal/worker/main.py      # Load app, execute routes
python/neutrino/internal/worker/protocol.py  # send_route_registry
```

---

## Testing Strategy

### Phase 1: Protocol
```bash
# Unit test: Message serialization round-trip
cargo test protocol
```

### Phase 2: Worker App Loading
```python
# tests/test_worker_loading.py
# Test that worker can import myapp:app and list routes
```

### Phase 3: HTTP Server
```bash
# Start server with mock orchestrator
# curl -X POST http://localhost:8000/test
```

### Phase 4: CLI
```bash
# Test argument parsing
cargo run -- serve --help
cargo run -- serve myapp:app --workers 2 --port 9000
```

### Phase 5: End-to-End
```bash
# Create test app
# python/tests/fixtures/myapp.py
from neutrino import App
app = App()

@app.route("/echo")
def echo(msg: str):
    return {"echo": msg}

# Run server
$ cd python && PYTHONPATH=. cargo run --manifest-path ../crates/neutrino-core/Cargo.toml -- serve tests.fixtures.myapp:app

# Test request
$ curl -X POST http://localhost:8000/echo -d '{"msg": "hello"}'
{"echo": "hello"}
```

---

## Error Handling

### HTTP Layer
- 404: Route not found
- 405: Method not allowed
- 500: Worker execution error
- 503: No workers available

### Worker Layer
- Import errors → Log and exit
- Route not found → Return error in TaskResult
- Execution exception → Catch, return error in TaskResult

### Orchestrator Layer
- Worker death → Remove from pool, spawn replacement
- All workers busy → Queue request or return 503

---

## Future Considerations

1. **Request queuing**: When all workers busy, queue requests
2. **Async routes**: Support `async def` route handlers
3. **Request validation**: Pydantic models for input validation
4. **Middleware**: Before/after hooks for routes
5. **Health checks**: `/health` endpoint
6. **Metrics**: Prometheus endpoint at `/metrics`
7. **Graceful shutdown**: Drain requests before stopping
