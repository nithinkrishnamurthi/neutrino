use axum::{
    body::Body,
    extract::State,
    http::{Request, Response, StatusCode},
    response::IntoResponse,
};
use std::sync::Arc;
use std::time::Instant;
use tracing::{error, info};
use uuid::Uuid;

use crate::db_logger::{DbLogger, LogEntry};

#[derive(Clone)]
pub struct AppState {
    pub backend_url: String,
    pub http_client: reqwest::Client,
    pub db_logger: Arc<DbLogger>,
}

/// Proxy handler that forwards requests to the backend and logs to database
pub async fn proxy_handler(
    State(state): State<AppState>,
    req: Request<Body>,
) -> Result<Response<Body>, ProxyError> {
    let task_id = Uuid::new_v4().to_string();
    let method = req.method().clone();
    let path = req.uri().path().to_string();
    let query = req
        .uri()
        .query()
        .map(|q| format!("?{}", q))
        .unwrap_or_default();

    // Extract function name from path (e.g., /api/function_name -> function_name)
    let function_name = extract_function_name(&path);

    info!(
        "Proxying request: {} {} (task_id: {})",
        method, path, task_id
    );

    // Capture request body
    let (parts, body) = req.into_parts();
    let body_bytes = match axum::body::to_bytes(body, usize::MAX).await {
        Ok(bytes) => bytes,
        Err(e) => {
            error!("Failed to read request body: {}", e);
            return Err(ProxyError::BodyReadError(e.to_string()));
        }
    };

    let request_body = String::from_utf8_lossy(&body_bytes).to_string();
    let created_at = chrono::Utc::now().to_rfc3339();

    // Log request start (non-blocking)
    state.db_logger.log(LogEntry {
        id: task_id.clone(),
        function_name: Some(function_name.clone()),
        method: method.to_string(),
        path: path.clone(),
        status: "started".to_string(),
        created_at: Some(created_at.clone()),
        request_body: Some(truncate_body(&request_body, 10000)),
        ..Default::default()
    });

    let start = Instant::now();

    // Build target URL
    let target_url = format!("{}{}{}", state.backend_url, path, query);

    // Build proxy request
    let mut proxy_req = state
        .http_client
        .request(method.clone(), &target_url)
        .body(body_bytes.to_vec());

    // Forward headers (except host and content-length which reqwest handles)
    for (key, value) in parts.headers.iter() {
        let key_str = key.as_str();
        if key_str != "host" && key_str != "content-length" {
            proxy_req = proxy_req.header(key, value);
        }
    }

    // Send request to backend
    let proxy_resp = match proxy_req.send().await {
        Ok(resp) => resp,
        Err(e) => {
            error!("Failed to send request to backend: {}", e);

            let duration_ms = start.elapsed().as_millis() as f64;

            // Log failure - preserve created_at from initial log
            state.db_logger.log(LogEntry {
                id: task_id,
                function_name: Some(function_name),
                method: method.to_string(),
                path,
                status: "failed".to_string(),
                created_at: Some(created_at),
                completed_at: Some(chrono::Utc::now().to_rfc3339()),
                duration_ms: Some(duration_ms),
                request_body: Some(truncate_body(&request_body, 10000)),
                error: Some(format!("Backend error: {}", e)),
                ..Default::default()
            });

            return Err(ProxyError::BackendError(e.to_string()));
        }
    };

    // Capture response
    let status = proxy_resp.status();
    let headers = proxy_resp.headers().clone();
    let resp_bytes = match proxy_resp.bytes().await {
        Ok(bytes) => bytes,
        Err(e) => {
            error!("Failed to read response body: {}", e);
            return Err(ProxyError::BodyReadError(e.to_string()));
        }
    };

    let response_body = String::from_utf8_lossy(&resp_bytes).to_string();
    let duration_ms = start.elapsed().as_millis() as f64;

    // Log completion (non-blocking) - preserve created_at from initial log
    state.db_logger.log(LogEntry {
        id: task_id.clone(),
        function_name: Some(function_name),
        method: method.to_string(),
        path,
        status: if status.is_success() {
            "completed".to_string()
        } else {
            "failed".to_string()
        },
        created_at: Some(created_at.clone()),
        completed_at: Some(chrono::Utc::now().to_rfc3339()),
        duration_ms: Some(duration_ms),
        status_code: Some(status.as_u16()),
        request_body: Some(truncate_body(&request_body, 10000)),
        response_body: Some(truncate_body(&response_body, 10000)),
        error: if !status.is_success() {
            Some(format!("HTTP {}", status.as_u16()))
        } else {
            None
        },
        ..Default::default()
    });

    info!(
        "Request completed: {} (status: {}, duration: {:.2}ms)",
        task_id.clone(), status, duration_ms
    );

    // Build response
    let mut response = Response::builder().status(status);

    // Copy headers from backend response
    for (key, value) in headers.iter() {
        response = response.header(key, value);
    }

    let response = response
        .body(Body::from(resp_bytes.to_vec()))
        .map_err(|e| ProxyError::ResponseBuildError(e.to_string()))?;

    Ok(response)
}

/// Extract function name from path
/// E.g., /api/function_name -> function_name
fn extract_function_name(path: &str) -> String {
    path.trim_start_matches('/')
        .split('/')
        .last()
        .unwrap_or("unknown")
        .to_string()
}

/// Truncate body for storage (to avoid storing huge responses)
fn truncate_body(body: &str, max_len: usize) -> String {
    if body.len() > max_len {
        format!("{}... (truncated)", &body[..max_len])
    } else {
        body.to_string()
    }
}

/// Custom error type for proxy errors
#[derive(Debug)]
pub enum ProxyError {
    BodyReadError(String),
    BackendError(String),
    ResponseBuildError(String),
}

impl IntoResponse for ProxyError {
    fn into_response(self) -> Response<Body> {
        let (status, message) = match self {
            ProxyError::BodyReadError(e) => (
                StatusCode::BAD_REQUEST,
                format!("Failed to read request body: {}", e),
            ),
            ProxyError::BackendError(e) => (
                StatusCode::BAD_GATEWAY,
                format!("Backend error: {}", e),
            ),
            ProxyError::ResponseBuildError(e) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to build response: {}", e),
            ),
        };

        let body = serde_json::json!({
            "error": message,
        });

        (status, axum::Json(body)).into_response()
    }
}
