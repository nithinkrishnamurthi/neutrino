use axum::{
    body::Body,
    extract::{State, Request},
    http::StatusCode,
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::{any, get, post, delete, patch, put},
    Extension, Json, Router,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::Duration;
use tracing::{info, warn};

use crate::config::AsgiConfig;
use crate::openapi::OpenApiSpec;
use crate::orchestrator::Orchestrator;
use crate::protocol::Message;

/// Shared application state
#[derive(Clone)]
pub struct AppState {
    pub orchestrator: Arc<Orchestrator>,
    pub asgi_config: Option<AsgiConfig>,
    pub asgi_client: Option<reqwest::Client>,
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

/// Execute a task with no request body (for GET/DELETE requests)
async fn execute_task_no_body(
    State(state): State<AppState>,
    Extension(handler_name): Extension<String>,
) -> Result<Json<TaskResponse>, AppError> {
    info!("Received request for handler: {}", handler_name);

    let start = std::time::Instant::now();

    // Get next available worker using round-robin
    let worker_idx = state
        .orchestrator
        .get_next_worker()
        .await
        .ok_or_else(|| AppError::NoWorkersAvailable)?;

    let workers = state.orchestrator.workers();
    let mut workers_guard = workers.write().await;
    let worker = &mut workers_guard[worker_idx];

    info!(
        "Routing handler {} to worker {} (index {})",
        handler_name, worker.worker.id, worker_idx
    );

    // For GET/DELETE, send empty JSON object as args
    let args = serde_json::Value::Object(serde_json::Map::new());
    let args_bytes = rmp_serde::to_vec(&args)
        .map_err(|e| AppError::SerializationError(e.to_string()))?;

    // Create task assignment message
    let task_id = uuid::Uuid::new_v4().to_string();
    let msg = Message::TaskAssignment {
        task_id: task_id.clone(),
        function_name: handler_name.clone(),
        args: args_bytes,
    };

    // Send task to worker
    worker
        .send(&msg)
        .await
        .map_err(|e| AppError::WorkerCommunicationError(e.to_string()))?;

    // Mark worker as busy
    worker.worker.state = crate::worker::WorkerState::Busy;

    // Wait for result
    let result_msg = worker
        .recv()
        .await
        .map_err(|e| AppError::WorkerCommunicationError(e.to_string()))?;

    // Mark worker as idle again
    worker.worker.state = crate::worker::WorkerState::Idle;

    let execution_time = start.elapsed().as_millis() as u64;

    // Process result
    match result_msg {
        Message::TaskResult {
            success,
            result: result_bytes,
            ..
        } => {
            if success {
                let result: serde_json::Value = rmp_serde::from_slice(&result_bytes)
                    .map_err(|e| AppError::DeserializationError(e.to_string()))?;

                Ok(Json(TaskResponse {
                    success: true,
                    result: Some(result),
                    error: None,
                    worker_id: Some(worker.worker.id.clone()),
                    execution_time_ms: Some(execution_time),
                }))
            } else {
                let error: serde_json::Value = rmp_serde::from_slice(&result_bytes)
                    .map_err(|e| AppError::DeserializationError(e.to_string()))?;

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
    Extension(handler_name): Extension<String>,
    Json(request): Json<TaskRequest>,
) -> Result<Json<TaskResponse>, AppError> {
    info!("Received request for handler: {}", handler_name);

    let start = std::time::Instant::now();

    // Get next available worker using round-robin
    let worker_idx = state
        .orchestrator
        .get_next_worker()
        .await
        .ok_or_else(|| AppError::NoWorkersAvailable)?;

    let workers = state.orchestrator.workers();
    let mut workers_guard = workers.write().await;
    let worker = &mut workers_guard[worker_idx];

    info!(
        "Routing handler {} to worker {} (index {})",
        handler_name, worker.worker.id, worker_idx
    );

    // Serialize arguments to msgpack
    let args_bytes = rmp_serde::to_vec(&request.args)
        .map_err(|e| AppError::SerializationError(e.to_string()))?;

    // Create task assignment message
    let task_id = uuid::Uuid::new_v4().to_string();
    let msg = Message::TaskAssignment {
        task_id: task_id.clone(),
        function_name: handler_name.clone(),
        args: args_bytes,
    };

    // Send task to worker
    worker
        .send(&msg)
        .await
        .map_err(|e| AppError::WorkerCommunicationError(e.to_string()))?;

    // Mark worker as busy
    worker.worker.state = crate::worker::WorkerState::Busy;

    // Wait for result
    let result_msg = worker
        .recv()
        .await
        .map_err(|e| AppError::WorkerCommunicationError(e.to_string()))?;

    // Mark worker as idle again
    worker.worker.state = crate::worker::WorkerState::Idle;

    let execution_time = start.elapsed().as_millis() as u64;

    // Process result
    match result_msg {
        Message::TaskResult {
            success,
            result: result_bytes,
            ..
        } => {
            if success {
                let result: serde_json::Value = rmp_serde::from_slice(&result_bytes)
                    .map_err(|e| AppError::DeserializationError(e.to_string()))?;

                Ok(Json(TaskResponse {
                    success: true,
                    result: Some(result),
                    error: None,
                    worker_id: Some(worker.worker.id.clone()),
                    execution_time_ms: Some(execution_time),
                }))
            } else {
                let error: serde_json::Value = rmp_serde::from_slice(&result_bytes)
                    .map_err(|e| AppError::DeserializationError(e.to_string()))?;

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

/// Proxy handler for ASGI routes
async fn proxy_to_asgi(
    State(state): State<AppState>,
    req: Request,
) -> Result<Response, AppError> {
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

    // Get the original URI and strip the ASGI prefix
    let original_path = req.uri().path();
    let query = req.uri().query().map(|q| format!("?{}", q)).unwrap_or_default();

    // Build target URL
    let target_url = format!("{}{}{}", target_base, original_path, query);

    info!("Proxying ASGI request: {} -> {}", original_path, target_url);

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

    let state = AppState {
        orchestrator,
        asgi_config: asgi_config.clone(),
        asgi_client,
    };

    let mut router = Router::new()
        .route("/health", get(health_check))
        .route("/status", get(get_status));

    // If OpenAPI spec is provided, create dynamic routes
    if let Some(spec) = openapi_spec {
        info!("Loading routes from OpenAPI specification");
        let routes = spec.extract_routes();

        for route_info in routes {
            info!(
                "Registering route: {} {} -> {}",
                route_info.method, route_info.path, route_info.handler_name
            );

            // Create a middleware that injects the handler name as an extension
            let handler_name = route_info.handler_name.clone();
            let handler_middleware = middleware::from_fn(move |mut req: Request, next: Next| {
                let handler_name = handler_name.clone();
                async move {
                    req.extensions_mut().insert(handler_name);
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

    // Add ASGI proxy route if configured
    if let Some(ref config) = asgi_config {
        if config.enabled {
            let path_prefix = config.path_prefix.clone();
            info!("Mounting ASGI proxy at path prefix: {}", path_prefix);

            // Create catch-all route for ASGI prefix
            let asgi_route = format!("{}/*path", path_prefix.trim_end_matches('/'));
            router = router.route(&asgi_route, any(proxy_to_asgi));

            // Also handle exact prefix path (without trailing slash)
            router = router.route(&path_prefix, any(proxy_to_asgi));
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
