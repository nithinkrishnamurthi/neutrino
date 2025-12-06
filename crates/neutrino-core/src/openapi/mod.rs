use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::Path;

use crate::protocol::ResourceRequirements;

/// OpenAPI 3.0 specification
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct OpenApiSpec {
    pub openapi: String,
    pub info: Info,
    pub paths: HashMap<String, PathItem>,
    #[serde(default)]
    pub components: Components,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Info {
    pub title: String,
    pub version: String,
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct Components {
    #[serde(default)]
    pub schemas: HashMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct PathItem {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub get: Option<Operation>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub post: Option<Operation>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub put: Option<Operation>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub patch: Option<Operation>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub delete: Option<Operation>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Operation {
    pub operation_id: String,
    #[serde(default)]
    pub summary: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub parameters: Vec<Parameter>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub request_body: Option<RequestBody>,
    #[serde(default)]
    pub responses: HashMap<String, Response>,
    /// Neutrino-specific resource requirements (OpenAPI extension field)
    #[serde(rename = "x-neutrino-resources", skip_serializing_if = "Option::is_none")]
    pub neutrino_resources: Option<ResourceRequirements>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Parameter {
    pub name: String,
    #[serde(rename = "in")]
    pub location: String,
    #[serde(default)]
    pub required: bool,
    pub schema: serde_json::Value,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RequestBody {
    #[serde(default)]
    pub required: bool,
    pub content: HashMap<String, MediaType>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Response {
    pub description: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<HashMap<String, MediaType>>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct MediaType {
    pub schema: serde_json::Value,
}

/// Route information extracted from OpenAPI spec
#[derive(Debug, Clone)]
pub struct RouteInfo {
    pub path: String,
    pub method: String,
    pub operation_id: String,
    pub handler_name: String,
    pub resources: ResourceRequirements,
}

impl OpenApiSpec {
    /// Load OpenAPI spec from a JSON file
    pub fn from_file<P: AsRef<Path>>(path: P) -> Result<Self, Box<dyn std::error::Error>> {
        let content = fs::read_to_string(path)?;
        let spec: OpenApiSpec = serde_json::from_str(&content)?;
        Ok(spec)
    }

    /// Extract all routes from the OpenAPI spec
    pub fn extract_routes(&self) -> Vec<RouteInfo> {
        let mut routes = Vec::new();

        for (path, path_item) in &self.paths {
            // Convert OpenAPI path format {param} to Axum format :param
            let axum_path = convert_openapi_path_to_axum(path);

            if let Some(op) = &path_item.get {
                routes.push(RouteInfo {
                    path: axum_path.clone(),
                    method: "GET".to_string(),
                    operation_id: op.operation_id.clone(),
                    handler_name: extract_handler_name(&op.operation_id),
                    resources: op.neutrino_resources.clone().unwrap_or_default(),
                });
            }

            if let Some(op) = &path_item.post {
                routes.push(RouteInfo {
                    path: axum_path.clone(),
                    method: "POST".to_string(),
                    operation_id: op.operation_id.clone(),
                    handler_name: extract_handler_name(&op.operation_id),
                    resources: op.neutrino_resources.clone().unwrap_or_default(),
                });
            }

            if let Some(op) = &path_item.put {
                routes.push(RouteInfo {
                    path: axum_path.clone(),
                    method: "PUT".to_string(),
                    operation_id: op.operation_id.clone(),
                    handler_name: extract_handler_name(&op.operation_id),
                    resources: op.neutrino_resources.clone().unwrap_or_default(),
                });
            }

            if let Some(op) = &path_item.patch {
                routes.push(RouteInfo {
                    path: axum_path.clone(),
                    method: "PATCH".to_string(),
                    operation_id: op.operation_id.clone(),
                    handler_name: extract_handler_name(&op.operation_id),
                    resources: op.neutrino_resources.clone().unwrap_or_default(),
                });
            }

            if let Some(op) = &path_item.delete {
                routes.push(RouteInfo {
                    path: axum_path.clone(),
                    method: "DELETE".to_string(),
                    operation_id: op.operation_id.clone(),
                    handler_name: extract_handler_name(&op.operation_id),
                    resources: op.neutrino_resources.clone().unwrap_or_default(),
                });
            }
        }

        routes
    }
}

/// Convert OpenAPI path format to Axum path format
/// Example: /users/{user_id} -> /users/:user_id
fn convert_openapi_path_to_axum(path: &str) -> String {
    let mut result = String::new();
    let mut in_param = false;
    let mut param_name = String::new();

    for ch in path.chars() {
        match ch {
            '{' => {
                in_param = true;
                param_name.clear();
                result.push(':');
            }
            '}' => {
                in_param = false;
                result.push_str(&param_name);
            }
            _ => {
                if in_param {
                    param_name.push(ch);
                } else {
                    result.push(ch);
                }
            }
        }
    }

    result
}

/// Extract handler name from operation ID
/// Example: "get_list_users" -> "list_users"
fn extract_handler_name(operation_id: &str) -> String {
    // Remove method prefix (get_, post_, put_, patch_, delete_)
    operation_id
        .strip_prefix("get_")
        .or_else(|| operation_id.strip_prefix("post_"))
        .or_else(|| operation_id.strip_prefix("put_"))
        .or_else(|| operation_id.strip_prefix("patch_"))
        .or_else(|| operation_id.strip_prefix("delete_"))
        .unwrap_or(operation_id)
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_convert_openapi_path_to_axum() {
        assert_eq!(
            convert_openapi_path_to_axum("/users/{user_id}"),
            "/users/:user_id"
        );
        assert_eq!(
            convert_openapi_path_to_axum("/api/v1/items/{item_id}/details/{detail_id}"),
            "/api/v1/items/:item_id/details/:detail_id"
        );
        assert_eq!(
            convert_openapi_path_to_axum("/health"),
            "/health"
        );
    }

    #[test]
    fn test_extract_handler_name() {
        assert_eq!(extract_handler_name("get_list_users"), "list_users");
        assert_eq!(extract_handler_name("post_create_user"), "create_user");
        assert_eq!(extract_handler_name("custom_handler"), "custom_handler");
    }
}
