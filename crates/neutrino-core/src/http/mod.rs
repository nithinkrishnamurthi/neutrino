use axum::{
    body::Body,
    extract::{State, Request},
    http::StatusCode,
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::{get, post, delete, patch, put},
    Extension, Json, Router,
};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::sync::Arc;
use std::time::Duration;
use tracing::{info, warn};

use crate::config::AsgiConfig;
use crate::openapi::OpenApiSpec;
use crate::orchestrator::Orchestrator;
use crate::protocol::Message;

use crate::protocol::ResourceRequirements;

/// Shared application state
#[derive(Clone)]
pub struct AppState {
    pub orchestrator: Arc<Orchestrator>,
    pub asgi_config: Option<AsgiConfig>,
    pub asgi_client: Option<reqwest::Client>,
    /// Set of registered Neutrino route paths for lookup-based routing
    pub neutrino_routes: Arc<HashSet<String>>,
}

/// Route metadata passed through request extensions
#[derive(Clone, Debug)]
pub struct RouteMetadata {
    pub handler_name: String,
    pub resources: ResourceRequirements,
}

/// Request body for task execution
#[derive(Debug, Deserialize)]
pub struct TaskRequest {
    pub args: serde_json::Value,
}

/// Response for task execution
#[derive(Debug, Serialize)]
pub struct TaskResponse {
    pub success: bool,
    pub result: Option<serde_json::Value>,
    pub error: Option<String>,
    pub worker_id: Option<String>,
    pub execution_time_ms: Option<u64>,
}

/// Convert serde_json::Value to rmpv::Value
fn json_to_msgpack_value(json: &serde_json::Value) -> Result<rmpv::Value, String> {
    match json {
        serde_json::Value::Null => Ok(rmpv::Value::Nil),
        serde_json::Value::Bool(b) => Ok(rmpv::Value::Boolean(*b)),
        serde_json::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                Ok(rmpv::Value::Integer(i.into()))
            } else if let Some(f) = n.as_f64() {
                Ok(rmpv::Value::F64(f))
            } else {
                Err("Invalid number".to_string())
            }
        }
        serde_json::Value::String(s) => Ok(rmpv::Value::String(s.clone().into())),
        serde_json::Value::Array(arr) => {
            let values: Result<Vec<_>, _> = arr.iter().map(json_to_msgpack_value).collect();
            Ok(rmpv::Value::Array(values?))
        }
        serde_json::Value::Object(obj) => {
            let pairs: Result<Vec<(rmpv::Value, rmpv::Value)>, String> = obj
                .iter()
                .map(|(k, v)| {
                    Ok((
                        rmpv::Value::String(k.clone().into()),
                        json_to_msgpack_value(v)?,
                    ))
                })
                .collect();
            Ok(rmpv::Value::Map(pairs?))
        }
    }
}

/// Convert rmpv::Value to serde_json::Value
fn msgpack_value_to_json(msgpack: &rmpv::Value) -> Result<serde_json::Value, String> {
    match msgpack {
        rmpv::Value::Nil => Ok(serde_json::Value::Null),
        rmpv::Value::Boolean(b) => Ok(serde_json::Value::Bool(*b)),
        rmpv::Value::Integer(i) => {
            if let Some(val) = i.as_i64() {
                Ok(serde_json::json!(val))
            } else if let Some(val) = i.as_u64() {
                Ok(serde_json::json!(val))
            } else {
                Err("Integer out of range".to_string())
            }
        }
        rmpv::Value::F32(f) => Ok(serde_json::json!(*f)),
        rmpv::Value::F64(f) => Ok(serde_json::json!(*f)),
        rmpv::Value::String(s) => Ok(serde_json::Value::String(
            s.as_str().ok_or("Invalid UTF-8")?.to_string(),
        )),
        rmpv::Value::Binary(b) => {
            // Convert binary to array of numbers for JSON compatibility
            Ok(serde_json::Value::Array(
                b.iter().map(|&byte| serde_json::json!(byte)).collect(),
            ))
        }
        rmpv::Value::Array(arr) => {
            let values: Result<Vec<_>, _> = arr.iter().map(msgpack_value_to_json).collect();
            Ok(serde_json::Value::Array(values?))
        }
        rmpv::Value::Map(map) => {
            let mut obj = serde_json::Map::new();
            for (k, v) in map {
                let key = match k {
                    rmpv::Value::String(s) => s.as_str().ok_or("Invalid UTF-8")?.to_string(),
                    _ => return Err("Map keys must be strings".to_string()),
                };
                obj.insert(key, msgpack_value_to_json(v)?);
            }
            Ok(serde_json::Value::Object(obj))
        }
        rmpv::Value::Ext(_, _) => Err("Extension types not supported".to_string()),
    }
}

/// Health check endpoint
async fn health_check() -> impl IntoResponse {
    Json(serde_json::json!({
        "status": "healthy",
        "service": "neutrino-orchestrator"
    }))
}

/// Get orchestrator status
async fn get_status(State(state): State<AppState>) -> impl IntoResponse {
    let worker_count = state.orchestrator.worker_count().await;

    Json(serde_json::json!({
        "status": "running",
        "workers": {
            "active": worker_count,
        }
    }))
}

/// Get resource capacity information for all workers
async fn get_capacity(State(state): State<AppState>) -> impl IntoResponse {
    let workers = state.orchestrator.workers();
    let workers_guard = workers.read().await;

    let mut worker_capacities = Vec::new();
    let mut total_cpus = 0.0;
    let mut total_gpus = 0.0;
    let mut total_memory_gb = 0.0;
    let mut available_cpus = 0.0;
    let mut available_gpus = 0.0;
    let mut available_memory_gb = 0.0;

    for worker_handle in workers_guard.iter() {
        let worker = &worker_handle.worker;
        let (avail_cpu, avail_gpu, avail_mem) = worker.available_resources();

        worker_capacities.push(serde_json::json!({
            "worker_id": worker.id,
            "state": format!("{:?}", worker.state),
            "capabilities": {
                "cpus": worker.capabilities.num_cpus,
                "gpus": worker.capabilities.num_gpus,
                "memory_gb": worker.capabilities.memory_gb,
            },
            "allocated": {
                "cpus": worker.allocation.allocated_cpus,
                "gpus": worker.allocation.allocated_gpus,
                "memory_gb": worker.allocation.allocated_memory_gb,
            },
            "available": {
                "cpus": avail_cpu,
                "gpus": avail_gpu,
                "memory_gb": avail_mem,
            },
        }));

        total_cpus += worker.capabilities.num_cpus;
        total_gpus += worker.capabilities.num_gpus;
        total_memory_gb += worker.capabilities.memory_gb;
        available_cpus += avail_cpu;
        available_gpus += avail_gpu;
        available_memory_gb += avail_mem;
    }

    Json(serde_json::json!({
        "total": {
            "cpus": total_cpus,
            "gpus": total_gpus,
            "memory_gb": total_memory_gb,
        },
        "available": {
            "cpus": available_cpus,
            "gpus": available_gpus,
            "memory_gb": available_memory_gb,
        },
        "allocated": {
            "cpus": total_cpus - available_cpus,
            "gpus": total_gpus - available_gpus,
            "memory_gb": total_memory_gb - available_memory_gb,
        },
        "workers": worker_capacities,
    }))
}

/// Execute a task with no request body (for GET/DELETE requests)
async fn execute_task_no_body(
    State(state): State<AppState>,
    Extension(metadata): Extension<RouteMetadata>,
) -> Result<Json<TaskResponse>, AppError> {
    info!("Received request for handler: {}", metadata.handler_name);

    let start = std::time::Instant::now();

    // Find worker with sufficient resources
    let worker_idx = state
        .orchestrator
        .find_worker_with_resources(&metadata.resources)
        .await
        .ok_or_else(|| AppError::InsufficientResources(format!(
            "No workers available with required resources: cpus={}, gpus={}, memory={}GB",
            metadata.resources.num_cpus,
            metadata.resources.num_gpus,
            metadata.resources.memory_gb
        )))?;

    let workers = state.orchestrator.workers();
    let mut workers_guard = workers.write().await;
    let worker = &mut workers_guard[worker_idx];

    info!(
        "Routing handler {} to worker {} (index {}) with resources: cpus={}, gpus={}, mem={}GB",
        metadata.handler_name,
        worker.worker.id,
        worker_idx,
        metadata.resources.num_cpus,
        metadata.resources.num_gpus,
        metadata.resources.memory_gb
    );

    // Allocate resources
    worker.worker.allocation.allocate(&metadata.resources);

    // For GET/DELETE, send empty map as args
    let args = rmpv::Value::Map(vec![]);

    // Create task assignment message
    let task_id = uuid::Uuid::new_v4().to_string();
    let msg = Message::TaskAssignment {
        task_id: task_id.clone(),
        function_name: metadata.handler_name.clone(),
        args,
        resources: metadata.resources.clone(),
    };

    // Send task to worker
    worker
        .send(&msg)
        .await
        .map_err(|e| {
            // Deallocate on error
            worker.worker.allocation.deallocate(&metadata.resources);
            AppError::WorkerCommunicationError(e.to_string())
        })?;

    // Mark worker as busy
    worker.worker.state = crate::worker::WorkerState::Busy;

    // Wait for result
    let result_msg = worker
        .recv()
        .await
        .map_err(|e| {
            // Deallocate on error
            worker.worker.allocation.deallocate(&metadata.resources);
            worker.worker.state = crate::worker::WorkerState::Idle;
            AppError::WorkerCommunicationError(e.to_string())
        })?;

    // Deallocate resources after task completion
    worker.worker.allocation.deallocate(&metadata.resources);

    // Mark worker as idle again
    worker.worker.state = crate::worker::WorkerState::Idle;

    let execution_time = start.elapsed().as_millis() as u64;

    // Process result
    match result_msg {
        Message::TaskResult {
            success,
            result: result_value,
            ..
        } => {
            if success {
                let result = msgpack_value_to_json(&result_value)
                    .map_err(|e| AppError::DeserializationError(e))?;

                Ok(Json(TaskResponse {
                    success: true,
                    result: Some(result),
                    error: None,
                    worker_id: Some(worker.worker.id.clone()),
                    execution_time_ms: Some(execution_time),
                }))
            } else {
                let error = msgpack_value_to_json(&result_value)
                    .map_err(|e| AppError::DeserializationError(e))?;

                Ok(Json(TaskResponse {
                    success: false,
                    result: None,
                    error: Some(error.to_string()),
                    worker_id: Some(worker.worker.id.clone()),
                    execution_time_ms: Some(execution_time),
                }))
            }
        }
        _ => Err(AppError::UnexpectedResponse),
    }
}

/// Execute a task with JSON request body (for POST/PUT/PATCH requests)
async fn execute_task_with_body(
    State(state): State<AppState>,
    Extension(metadata): Extension<RouteMetadata>,
    Json(request): Json<TaskRequest>,
) -> Result<Json<TaskResponse>, AppError> {
    info!("Received request for handler: {}", metadata.handler_name);

    let start = std::time::Instant::now();

    // Find worker with sufficient resources
    let worker_idx = state
        .orchestrator
        .find_worker_with_resources(&metadata.resources)
        .await
        .ok_or_else(|| AppError::InsufficientResources(format!(
            "No workers available with required resources: cpus={}, gpus={}, memory={}GB",
            metadata.resources.num_cpus,
            metadata.resources.num_gpus,
            metadata.resources.memory_gb
        )))?;

    let workers = state.orchestrator.workers();
    let mut workers_guard = workers.write().await;
    let worker = &mut workers_guard[worker_idx];

    info!(
        "Routing handler {} to worker {} (index {}) with resources: cpus={}, gpus={}, mem={}GB",
        metadata.handler_name,
        worker.worker.id,
        worker_idx,
        metadata.resources.num_cpus,
        metadata.resources.num_gpus,
        metadata.resources.memory_gb
    );

    // Allocate resources
    worker.worker.allocation.allocate(&metadata.resources);

    // Convert JSON to msgpack Value
    let args = json_to_msgpack_value(&request.args)
        .map_err(|e| {
            // Deallocate on error
            worker.worker.allocation.deallocate(&metadata.resources);
            AppError::SerializationError(e.to_string())
        })?;

    // Create task assignment message
    let task_id = uuid::Uuid::new_v4().to_string();
    let msg = Message::TaskAssignment {
        task_id: task_id.clone(),
        function_name: metadata.handler_name.clone(),
        args,
        resources: metadata.resources.clone(),
    };

    // Send task to worker
    worker
        .send(&msg)
        .await
        .map_err(|e| {
            // Deallocate on error
            worker.worker.allocation.deallocate(&metadata.resources);
            AppError::WorkerCommunicationError(e.to_string())
        })?;

    // Mark worker as busy
    worker.worker.state = crate::worker::WorkerState::Busy;

    // Wait for result
    let result_msg = worker
        .recv()
        .await
        .map_err(|e| {
            // Deallocate on error
            worker.worker.allocation.deallocate(&metadata.resources);
            worker.worker.state = crate::worker::WorkerState::Idle;
            AppError::WorkerCommunicationError(e.to_string())
        })?;

    // Deallocate resources after task completion
    worker.worker.allocation.deallocate(&metadata.resources);

    // Mark worker as idle again
    worker.worker.state = crate::worker::WorkerState::Idle;

    let execution_time = start.elapsed().as_millis() as u64;

    // Process result
    match result_msg {
        Message::TaskResult {
            success,
            result: result_value,
            ..
        } => {
            if success {
                let result = msgpack_value_to_json(&result_value)
                    .map_err(|e| AppError::DeserializationError(e))?;

                Ok(Json(TaskResponse {
                    success: true,
                    result: Some(result),
                    error: None,
                    worker_id: Some(worker.worker.id.clone()),
                    execution_time_ms: Some(execution_time),
                }))
            } else {
                let error = msgpack_value_to_json(&result_value)
                    .map_err(|e| AppError::DeserializationError(e))?;

                Ok(Json(TaskResponse {
                    success: false,
                    result: None,
                    error: Some(error.to_string()),
                    worker_id: Some(worker.worker.id.clone()),
                    execution_time_ms: Some(execution_time),
                }))
            }
        }
        _ => Err(AppError::UnexpectedResponse),
    }
}

/// Fallback handler that checks route lookup and proxies to ASGI if not found
async fn asgi_fallback_handler(
    State(state): State<AppState>,
    req: Request,
) -> Result<Response, AppError> {
    let path = req.uri().path();

    // Check if this route is registered in Neutrino
    if state.neutrino_routes.contains(path) {
        // This should never happen as registered routes are handled first
        // But if it does, return 500 to indicate routing misconfiguration
        return Err(AppError::RouteNotFound(path.to_string()));
    }

    // Route not in Neutrino - proxy to ASGI app
    let asgi_config = state
        .asgi_config
        .as_ref()
        .ok_or_else(|| AppError::AsgiNotConfigured)?;

    let client = state
        .asgi_client
        .as_ref()
        .ok_or_else(|| AppError::AsgiNotConfigured)?;

    // Determine target URL based on mode
    let target_base = match asgi_config.mode {
        crate::config::AsgiMode::Mounted => {
            format!("http://127.0.0.1:{}", asgi_config.port)
        }
        crate::config::AsgiMode::Proxy => {
            asgi_config
                .service_url
                .clone()
                .ok_or_else(|| AppError::AsgiConfigError(
                    "service_url required for proxy mode".to_string()
                ))?
        }
    };

    // Get the original URI
    let query = req.uri().query().map(|q| format!("?{}", q)).unwrap_or_default();

    // Build target URL
    let target_url = format!("{}{}{}", target_base, path, query);

    info!("Proxying to ASGI: {} -> {}", path, target_url);

    // Convert axum request to reqwest request
    let method = req.method().clone();
    let headers = req.headers().clone();
    let body_bytes = axum::body::to_bytes(req.into_body(), usize::MAX)
        .await
        .map_err(|e| AppError::ProxyError(format!("Failed to read request body: {}", e)))?;

    // Build reqwest request
    let mut proxy_req = client
        .request(method, &target_url)
        .timeout(Duration::from_secs(asgi_config.timeout_secs))
        .body(body_bytes.to_vec());

    // Forward headers (excluding host)
    for (key, value) in headers.iter() {
        if key != "host" {
            proxy_req = proxy_req.header(key, value);
        }
    }

    // Send request to ASGI app
    let proxy_resp = proxy_req
        .send()
        .await
        .map_err(|e| AppError::ProxyError(format!("ASGI request failed: {}", e)))?;

    // Convert reqwest response to axum response
    let status = proxy_resp.status();
    let headers = proxy_resp.headers().clone();
    let body_bytes = proxy_resp
        .bytes()
        .await
        .map_err(|e| AppError::ProxyError(format!("Failed to read ASGI response: {}", e)))?;

    let mut response = Response::builder().status(status);

    // Copy headers from ASGI response
    for (key, value) in headers.iter() {
        response = response.header(key, value);
    }

    let response = response
        .body(Body::from(body_bytes.to_vec()))
        .map_err(|e| AppError::ProxyError(format!("Failed to build response: {}", e)))?;

    Ok(response)
}

/// Custom error type
#[derive(Debug)]
pub enum AppError {
    NoWorkersAvailable,
    InsufficientResources(String),
    RouteNotFound(String),
    SerializationError(String),
    DeserializationError(String),
    WorkerCommunicationError(String),
    UnexpectedResponse,
    AsgiNotConfigured,
    AsgiConfigError(String),
    ProxyError(String),
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, message) = match self {
            AppError::NoWorkersAvailable => {
                (StatusCode::SERVICE_UNAVAILABLE, "No workers available".to_string())
            }
            AppError::InsufficientResources(details) => {
                (StatusCode::SERVICE_UNAVAILABLE, format!("Insufficient resources: {}", details))
            }
            AppError::RouteNotFound(route) => {
                (StatusCode::NOT_FOUND, format!("Route not found: {}", route))
            }
            AppError::SerializationError(e) => {
                (StatusCode::BAD_REQUEST, format!("Serialization error: {}", e))
            }
            AppError::DeserializationError(e) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Deserialization error: {}", e),
            ),
            AppError::WorkerCommunicationError(e) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Worker communication error: {}", e),
            ),
            AppError::UnexpectedResponse => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Unexpected response from worker".to_string(),
            ),
            AppError::AsgiNotConfigured => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "ASGI app not configured".to_string(),
            ),
            AppError::AsgiConfigError(e) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("ASGI configuration error: {}", e),
            ),
            AppError::ProxyError(e) => (
                StatusCode::BAD_GATEWAY,
                format!("Proxy error: {}", e),
            ),
        };

        let body = Json(serde_json::json!({
            "error": message,
        }));

        (status, body).into_response()
    }
}

/// Create the HTTP server router with optional OpenAPI spec for dynamic routing
pub fn create_router(orchestrator: Arc<Orchestrator>) -> Router {
    create_router_with_openapi(orchestrator, None, None)
}

/// Create the HTTP server router with OpenAPI spec and optional ASGI config
pub fn create_router_with_openapi(
    orchestrator: Arc<Orchestrator>,
    openapi_spec: Option<OpenApiSpec>,
    asgi_config: Option<AsgiConfig>,
) -> Router {
    // Create HTTP client for ASGI proxy if configured
    let asgi_client = if asgi_config.is_some() {
        Some(reqwest::Client::new())
    } else {
        None
    };

    // Build set of registered Neutrino routes for lookup
    let mut neutrino_routes = HashSet::new();
    neutrino_routes.insert("/health".to_string());
    neutrino_routes.insert("/status".to_string());
    neutrino_routes.insert("/capacity".to_string());

    let mut router = Router::new()
        .route("/health", get(health_check))
        .route("/status", get(get_status))
        .route("/capacity", get(get_capacity));

    // If OpenAPI spec is provided, create dynamic routes
    if let Some(spec) = openapi_spec {
        info!("Loading routes from OpenAPI specification");
        let routes = spec.extract_routes();

        for route_info in routes {
            info!(
                "Registering route: {} {} -> {} (cpus={}, gpus={}, mem={}GB)",
                route_info.method,
                route_info.path,
                route_info.handler_name,
                route_info.resources.num_cpus,
                route_info.resources.num_gpus,
                route_info.resources.memory_gb
            );

            // Add to Neutrino routes set
            neutrino_routes.insert(route_info.path.clone());

            // Create metadata with handler name and resource requirements
            let metadata = RouteMetadata {
                handler_name: route_info.handler_name.clone(),
                resources: route_info.resources.clone(),
            };

            // Create a middleware that injects the metadata as an extension
            let handler_middleware = middleware::from_fn(move |mut req: Request, next: Next| {
                let metadata = metadata.clone();
                async move {
                    req.extensions_mut().insert(metadata);
                    next.run(req).await
                }
            });

            // Create the method router based on the HTTP method with the middleware
            // Use execute_task_no_body for GET/DELETE, execute_task_with_body for POST/PUT/PATCH
            let method_router = match route_info.method.as_str() {
                "GET" => get(execute_task_no_body).layer(handler_middleware),
                "DELETE" => delete(execute_task_no_body).layer(handler_middleware),
                "POST" => post(execute_task_with_body).layer(handler_middleware),
                "PUT" => put(execute_task_with_body).layer(handler_middleware),
                "PATCH" => patch(execute_task_with_body).layer(handler_middleware),
                _ => {
                    warn!("Unsupported HTTP method: {}", route_info.method);
                    continue;
                }
            };

            router = router.route(&route_info.path, method_router);
        }
    } else {
        // Fallback to generic task route if no OpenAPI spec
        warn!("No OpenAPI spec provided - routes must be configured via OpenAPI");
        // Note: For production use, always provide an OpenAPI spec
    }

    let state = AppState {
        orchestrator,
        asgi_config: asgi_config.clone(),
        asgi_client,
        neutrino_routes: Arc::new(neutrino_routes),
    };

    // Add ASGI fallback handler if configured
    if let Some(ref config) = asgi_config {
        if config.enabled {
            info!("ASGI integration enabled - unmatched routes will fallback to ASGI app");

            // Add catch-all fallback route (lowest priority)
            router = router.fallback(asgi_fallback_handler);
        }
    }

    router.with_state(state)
}

/// Start the HTTP server
pub async fn start_server(
    orchestrator: Arc<Orchestrator>,
    host: String,
    port: u16,
) -> Result<(), Box<dyn std::error::Error>> {
    start_server_with_openapi(orchestrator, host, port, None, None).await
}

/// Start the HTTP server with optional OpenAPI spec path and ASGI config
pub async fn start_server_with_openapi(
    orchestrator: Arc<Orchestrator>,
    host: String,
    port: u16,
    openapi_path: Option<&str>,
    asgi_config: Option<AsgiConfig>,
) -> Result<(), Box<dyn std::error::Error>> {
    // Load OpenAPI spec if path is provided
    let openapi_spec = if let Some(path) = openapi_path {
        info!("Loading OpenAPI spec from: {}", path);
        match OpenApiSpec::from_file(path) {
            Ok(spec) => {
                info!("Successfully loaded OpenAPI spec: {} v{}", spec.info.title, spec.info.version);
                Some(spec)
            }
            Err(e) => {
                warn!("Failed to load OpenAPI spec: {}. Using fallback routing.", e);
                None
            }
        }
    } else {
        None
    };

    let app = create_router_with_openapi(orchestrator, openapi_spec, asgi_config);
    let addr = format!("{}:{}", host, port);

    info!("Starting HTTP server on {}", addr);

    let listener = tokio::net::TcpListener::bind(&addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}
