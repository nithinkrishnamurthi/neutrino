"""
Example: Integrating FastAPI with Neutrino

This example demonstrates how to mount an existing FastAPI application
alongside Neutrino routes. This is useful when you want to:

1. Gradually migrate from FastAPI to Neutrino
2. Use Neutrino for heavy compute tasks while keeping FastAPI for CRUD
3. Combine both frameworks in a single deployment

Usage:
    # 1. Generate OpenAPI spec and Uvicorn config:
    neutrino deploy examples.fastapi_integration --openapi

    # 2. Configure ASGI in config.yaml (see below)

    # 3. Start Neutrino:
    cargo run --release

Architecture:
    - Neutrino routes: /neutrino/*  → Handled by Rust orchestrator → Python workers
    - FastAPI routes:  /api/*       → Proxied to Uvicorn ASGI app
"""

from fastapi import FastAPI, HTTPException
from pydantic import BaseModel
from neutrino import App

# ============================================================================
# FastAPI Application (existing code, unchanged)
# ============================================================================

fastapi_app = FastAPI(
    title="Existing FastAPI App",
    description="Your existing FastAPI application",
    version="1.0.0"
)

class User(BaseModel):
    id: int
    name: str
    email: str

# Example: Simple CRUD endpoints
users_db = {
    1: User(id=1, name="Alice", email="alice@example.com"),
    2: User(id=2, name="Bob", email="bob@example.com"),
}

@fastapi_app.get("/health")
def health_check():
    """Health check endpoint for ASGI app"""
    return {"status": "healthy", "service": "fastapi"}

@fastapi_app.get("/api/users")
def list_users():
    """List all users"""
    return {"users": list(users_db.values())}

@fastapi_app.get("/users/{user_id}")
def get_user(user_id: int):
    """Get a specific user by ID"""
    if user_id not in users_db:
        raise HTTPException(status_code=404, detail="User not found")
    return users_db[user_id]

@fastapi_app.post("/users")
def create_user(user: User):
    """Create a new user"""
    if user.id in users_db:
        raise HTTPException(status_code=400, detail="User already exists")
    users_db[user.id] = user
    return {"message": "User created", "user": user}

@fastapi_app.put("/users/{user_id}")
def update_user(user_id: int, user: User):
    """Update an existing user"""
    if user_id not in users_db:
        raise HTTPException(status_code=404, detail="User not found")
    users_db[user_id] = user
    return {"message": "User updated", "user": user}

@fastapi_app.delete("/users/{user_id}")
def delete_user(user_id: int):
    """Delete a user"""
    if user_id not in users_db:
        raise HTTPException(status_code=404, detail="User not found")
    del users_db[user_id]
    return {"message": "User deleted"}

# ============================================================================
# Neutrino Application (heavy compute tasks)
# ============================================================================

app = App()

class TaskRequest(BaseModel):
    text: str
    iterations: int

class TaskResponse(BaseModel):
    result: str
    processed_chars: int

@app.route("/neutrino/process", methods=["POST"])
async def heavy_processing(request: TaskRequest) -> TaskResponse:
    """
    CPU-intensive task that benefits from Neutrino's worker pool.

    This would be slow in a standard async FastAPI setup because it
    blocks the event loop. With Neutrino, it runs in a separate worker
    process with proper resource management.
    """
    # Simulate heavy processing
    result = request.text
    for _ in range(request.iterations):
        result = result[::-1]  # Reverse string repeatedly

    return TaskResponse(
        result=result,
        processed_chars=len(result) * request.iterations
    )

class AnalysisRequest(BaseModel):
    user_id: int
    data: list[float]

class AnalysisResponse(BaseModel):
    user_id: int
    mean: float
    median: float
    std_dev: float

@app.route("/neutrino/analyze", methods=["POST"])
async def analyze_data(request: AnalysisRequest) -> AnalysisResponse:
    """
    Statistical analysis task.

    In a real application, this might involve ML inference, image processing,
    or other compute-heavy operations that benefit from Neutrino's
    orchestration and worker lifecycle management.
    """
    import statistics

    data = request.data
    return AnalysisResponse(
        user_id=request.user_id,
        mean=statistics.mean(data),
        median=statistics.median(data),
        std_dev=statistics.stdev(data) if len(data) > 1 else 0.0
    )

# ============================================================================
# Mount FastAPI into Neutrino
# ============================================================================

# This tells Neutrino to use FastAPI as a fallback for unmatched routes
app.mount_asgi(fastapi_app)

# ============================================================================
# Configuration
# ============================================================================

"""
Add this to your config.yaml:

orchestrator:
  app_module: "examples.fastapi_integration"

  # Enable ASGI integration
  asgi:
    enabled: true
    mode: "mounted"       # Use "mounted" for dev, "proxy" for production k8s
    port: 8081            # Internal Uvicorn port
    workers: 4            # Number of Uvicorn workers

  # ... rest of config ...

Then:
1. Generate deployment files:
   $ neutrino deploy examples.fastapi_integration --openapi

2. Start Neutrino:
   $ cargo run --release

3. Test endpoints:
   # FastAPI routes (handled by ASGI fallback):
   $ curl http://localhost:8080/api/users
   $ curl http://localhost:8080/api/users/1

   # Neutrino routes (handled by orchestrator):
   $ curl -X POST http://localhost:8080/neutrino/process \\
       -H "Content-Type: application/json" \\
       -d '{"text": "hello", "iterations": 1000000}'

   $ curl -X POST http://localhost:8080/neutrino/analyze \\
       -H "Content-Type: application/json" \\
       -d '{"user_id": 1, "data": [1.5, 2.3, 3.7, 4.2, 5.8]}'

How it works:
- Routes registered in Neutrino (@app.route) go to the orchestrator
- All other routes automatically fall through to the FastAPI app
- No prefix required - routes intermix naturally!
"""

# ============================================================================
# Kubernetes Deployment (Proxy Mode)
# ============================================================================

"""
For production Kubernetes deployment with separate services:

1. FastAPI Deployment (fastapi-deployment.yaml):
---
apiVersion: apps/v1
kind: Deployment
metadata:
  name: fastapi
spec:
  replicas: 3
  selector:
    matchLabels:
      app: fastapi
  template:
    metadata:
      labels:
        app: fastapi
    spec:
      containers:
      - name: fastapi
        image: your-registry/fastapi-app:latest
        ports:
        - containerPort: 8080
        command: ["uvicorn", "fastapi_integration:fastapi_app", "--host", "0.0.0.0", "--port", "8080"]
---
apiVersion: v1
kind: Service
metadata:
  name: fastapi-service
spec:
  selector:
    app: fastapi
  ports:
  - port: 8080
    targetPort: 8080

2. Neutrino Deployment (neutrino-deployment.yaml):
---
apiVersion: apps/v1
kind: Deployment
metadata:
  name: neutrino
spec:
  replicas: 2
  selector:
    matchLabels:
      app: neutrino
  template:
    metadata:
      labels:
        app: neutrino
    spec:
      containers:
      - name: neutrino
        image: your-registry/neutrino:latest
        ports:
        - containerPort: 8080
---
apiVersion: v1
kind: Service
metadata:
  name: neutrino-service
spec:
  selector:
    app: neutrino
  ports:
  - port: 8080
    targetPort: 8080

3. Neutrino config for proxy mode (in ConfigMap):
orchestrator:
  asgi:
    enabled: true
    mode: "proxy"
    service_url: "http://fastapi-service:8080"
    timeout_secs: 30

4. Ingress:
---
apiVersion: networking.k8s.io/v1
kind: Ingress
metadata:
  name: app-ingress
spec:
  rules:
  - http:
      paths:
      - path: /neutrino
        pathType: Prefix
        backend:
          service:
            name: neutrino-service
            port:
              number: 8080
      - path: /api
        pathType: Prefix
        backend:
          service:
            name: neutrino-service  # Neutrino proxies to fastapi-service
            port:
              number: 8080

With this setup:
- FastAPI scales independently based on CRUD traffic
- Neutrino scales independently based on compute workload
- Both can be updated/deployed separately
- Fault isolation: FastAPI issues don't affect Neutrino workers
"""
