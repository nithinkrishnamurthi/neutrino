# Neutrino OpenAPI Routing Example

This example demonstrates Neutrino's **exact route matching** via OpenAPI specification, replacing the previous generic task pattern with proper HTTP routing.

## Overview

**Before (Generic Pattern):**
```
/v1/tasks/:task_name  →  Any task name accepted
```

**After (Exact Routes from OpenAPI):**
```
GET  /api/users             →  list_users()
GET  /api/users/:user_id    →  get_user()
GET  /api/products          →  manage_products()
POST /api/products          →  manage_products()
```

## How It Works

### 1. Define Routes in Python

```python
from neutrino import App

app = App()

@app.route("/api/users", methods=["GET"])
def list_users():
    """List all users."""
    return {"users": [...]}

@app.route("/api/users/{user_id}", methods=["GET"])
def get_user(user_id: int):
    """Get a specific user by ID."""
    return {"id": user_id, "name": f"User {user_id}"}
```

### 2. Generate OpenAPI Spec

```bash
# Using Python directly
python -c "
from myapp import app
import json
spec = app.generate_openapi(title='My API', version='1.0.0')
with open('openapi.json', 'w') as f:
    json.dump(spec, f, indent=2)
"

# Or using Neutrino CLI
neutrino deploy myapp.main --openapi
```

This generates `openapi.json`:

```json
{
  "openapi": "3.0.0",
  "info": {
    "title": "My API",
    "version": "1.0.0"
  },
  "paths": {
    "/api/users": {
      "get": {
        "operationId": "get_list_users",
        "summary": "List Users",
        "description": "List all users.",
        ...
      }
    },
    "/api/users/{user_id}": {
      "get": {
        "operationId": "get_get_user",
        "summary": "Get User",
        "parameters": [
          {
            "name": "user_id",
            "in": "path",
            "required": true,
            "schema": {"type": "string"}
          }
        ],
        ...
      }
    }
  }
}
```

### 3. Rust Router Loads OpenAPI

When the Rust orchestrator starts, it:

1. **Loads** `openapi.json`
2. **Parses** OpenAPI paths and operations
3. **Converts** path format: `{user_id}` → `:user_id` (Axum format)
4. **Extracts** handler names from `operationId` (e.g., `get_list_users` → `list_users`)
5. **Registers** exact routes with Axum router

```rust
// In crates/neutrino-core/src/http/mod.rs
pub fn create_router_with_openapi(
    orchestrator: Arc<Orchestrator>,
    openapi_spec: Option<OpenApiSpec>,
) -> Router {
    // Dynamically creates routes like:
    //   GET  /api/users         → execute_task (with handler_name="list_users")
    //   GET  /api/users/:user_id → execute_task (with handler_name="get_user")
}
```

## Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                     Python Layer                            │
│                                                             │
│  @app.route("/api/users", methods=["GET"])                 │
│  def list_users():                                          │
│      return {"users": [...]}                                │
│                                                             │
│  ↓ app.generate_openapi()                                  │
└─────────────────────────────────────────────────────────────┘
                           │
                           ↓ openapi.json
┌─────────────────────────────────────────────────────────────┐
│                      Rust Layer                             │
│                                                             │
│  OpenApiSpec::from_file("openapi.json")                    │
│      ↓                                                      │
│  spec.extract_routes()                                      │
│      ↓                                                      │
│  [ RouteInfo {                                              │
│      path: "/api/users",                                    │
│      method: "GET",                                         │
│      handler_name: "list_users"                             │
│    }, ... ]                                                 │
│      ↓                                                      │
│  Router::new()                                              │
│      .route("/api/users", get(execute_task))               │
│      .layer(inject_handler_name("list_users"))             │
└─────────────────────────────────────────────────────────────┘
```

## Request Flow

```
1. HTTP Request
   GET /api/users

2. Axum Router
   Matches: GET /api/users
   Extension: handler_name = "list_users"

3. execute_task Handler
   - Extracts handler_name from request extension
   - Sends TaskAssignment message to Python worker:
     {
       "TaskAssignment": {
         "task_id": "uuid",
         "function_name": "list_users",  ← Exact function name!
         "args": {...}
       }
     }

4. Python Worker
   - Receives task
   - Calls app.get_route("/api/users").handler()
   - Returns result

5. Response
   JSON response to client
```

## Running the Example

### Generate OpenAPI Spec

```bash
cd /home/nithin/neutrino
python3 examples/test_routes.py
```

This will:
- Display registered routes
- Generate OpenAPI specification
- Save to `examples/openapi.json`

### Test OpenAPI Parsing (Rust)

```bash
cargo run --example test_openapi_routing
```

Expected output:
```
✓ Successfully loaded OpenAPI spec
  Title: Example API
  Version: 1.0.0

✓ Extracting routes...

Routes registered:
METHOD     PATH                                HANDLER
-----------------------------------------------------------------
GET        /api/users                          list_users
GET        /api/users/:user_id                 get_user
GET        /api/products                       manage_products
POST       /api/products                       manage_products

✓ Test passed: 4 routes extracted
✓ All route assertions passed!
🎉 OpenAPI-based routing is working correctly!
```

## Key Files

### Python Side
- `python/neutrino/route.py` - Route class with OpenAPI metadata support
- `python/neutrino/openapi_generator.py` - OpenAPI 3.0 spec generator
- `python/neutrino/app.py` - App.generate_openapi() method
- `python/neutrino/cli/main.py` - CLI `--openapi` flag

### Rust Side
- `crates/neutrino-core/src/openapi/mod.rs` - OpenAPI parser
- `crates/neutrino-core/src/http/mod.rs` - Dynamic router from OpenAPI

## Benefits

### ✅ Exact Route Matching
- No more generic `/v1/tasks/:task_name` pattern
- Each Python function gets its own HTTP route
- Proper REST API design

### ✅ Industry Standard
- OpenAPI 3.0 specification
- Compatible with Swagger UI, Postman, etc.
- Auto-generated API documentation

### ✅ Type Safety
- Route paths validated at deployment time
- Pydantic model integration (optional)
- Clear contract between Python and Rust

### ✅ Developer Experience
- FastAPI-like ergonomics
- Familiar `@app.route()` decorator
- Auto-generated docs from docstrings

## Comparison

### Before: Generic Task Pattern

```python
# Python
@app.task()
def process_data(data):
    return processed

# Request
POST /v1/tasks/process_data
{"args": {"data": ...}}
```

**Issues:**
- ❌ All tasks share one route
- ❌ Not RESTful
- ❌ Hard to document
- ❌ No path parameters

### After: OpenAPI-Based Exact Routes

```python
# Python
@app.route("/api/data/{data_id}", methods=["POST"])
def process_data(data_id: str):
    return processed

# Request
POST /api/data/123
{"args": {...}}
```

**Benefits:**
- ✅ Each function has its own route
- ✅ RESTful design
- ✅ Auto-documented via OpenAPI
- ✅ Path parameters supported

## Next Steps

1. **Add Pydantic models** for request/response validation:
   ```python
   from pydantic import BaseModel

   class UserResponse(BaseModel):
       id: int
       name: str

   @app.route("/api/users/{user_id}", response_model=UserResponse)
   def get_user(user_id: int) -> UserResponse:
       return UserResponse(id=user_id, name=f"User {user_id}")
   ```

2. **Generate Swagger UI** for interactive API documentation

3. **Add request validation** using Pydantic schemas from OpenAPI

4. **Support query parameters** and request bodies in OpenAPI spec
