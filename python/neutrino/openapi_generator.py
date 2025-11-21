"""
OpenAPI 3.0 specification generator for Neutrino routes.
"""

import inspect
import re
from typing import Any

from neutrino import App

try:
    from pydantic import BaseModel
    PYDANTIC_AVAILABLE = True
except ImportError:
    BaseModel = None  # type: ignore
    PYDANTIC_AVAILABLE = False


def convert_path_to_openapi(path: str) -> str:
    """
    Convert Neutrino path format to OpenAPI format.

    Examples:
        /users/{user_id} -> /users/{user_id}  (already OpenAPI format)
        /users/:user_id  -> /users/{user_id}  (convert from colon format)

    Args:
        path: The route path

    Returns:
        OpenAPI-formatted path
    """
    # Convert :param to {param} format
    return re.sub(r':(\w+)', r'{\1}', path)


def pydantic_model_to_schema(model: type) -> dict[str, Any]:
    """
    Convert a Pydantic model to OpenAPI schema.

    Args:
        model: Pydantic model class

    Returns:
        OpenAPI schema dictionary
    """
    if not PYDANTIC_AVAILABLE or not isinstance(model, type) or not issubclass(model, BaseModel):
        return {}

    try:
        # Pydantic v2 has model_json_schema()
        if hasattr(model, 'model_json_schema'):
            return model.model_json_schema()
        # Pydantic v1 has schema()
        elif hasattr(model, 'schema'):
            return model.schema()
    except Exception:
        pass

    return {}


def extract_path_parameters(path: str) -> list[dict[str, Any]]:
    """
    Extract path parameters from OpenAPI path.

    Args:
        path: OpenAPI path like /users/{user_id}

    Returns:
        List of parameter definitions
    """
    params = []
    # Match {param_name}
    for match in re.finditer(r'\{(\w+)\}', path):
        param_name = match.group(1)
        params.append({
            "name": param_name,
            "in": "path",
            "required": True,
            "schema": {"type": "string"},
        })
    return params


def generate_operation(route: Any, method: str) -> dict[str, Any]:
    """
    Generate OpenAPI operation object for a route method.

    Args:
        route: Route object
        method: HTTP method (GET, POST, etc.)

    Returns:
        OpenAPI operation dictionary
    """
    operation: dict[str, Any] = {
        "operationId": f"{method.lower()}_{route.handler.__name__}",
        "summary": route.summary,
        "tags": route.tags if route.tags else [],
    }

    if route.description:
        operation["description"] = route.description

    # Parameters (path params)
    openapi_path = convert_path_to_openapi(route.path)
    path_params = extract_path_parameters(openapi_path)
    if path_params:
        operation["parameters"] = path_params

    # Request body (for POST, PUT, PATCH)
    if method.upper() in ["POST", "PUT", "PATCH"] and route.request_model:
        schema = pydantic_model_to_schema(route.request_model)
        if schema:
            operation["requestBody"] = {
                "required": True,
                "content": {
                    "application/json": {
                        "schema": schema
                    }
                }
            }

    # Response
    responses: dict[str, Any] = {}

    if route.response_model:
        schema = pydantic_model_to_schema(route.response_model)
        if schema:
            responses["200"] = {
                "description": "Successful response",
                "content": {
                    "application/json": {
                        "schema": schema
                    }
                }
            }
    else:
        # Generic success response
        responses["200"] = {
            "description": "Successful response"
        }

    # Add common error responses
    responses["500"] = {
        "description": "Internal server error"
    }

    operation["responses"] = responses

    return operation


def generate_openapi_spec(app: App, title: str = "Neutrino API", version: str = "1.0.0") -> dict[str, Any]:
    """
    Generate OpenAPI 3.0 specification from Neutrino App.

    Args:
        app: Neutrino App instance
        title: API title
        version: API version

    Returns:
        OpenAPI 3.0 specification dictionary
    """
    spec: dict[str, Any] = {
        "openapi": "3.0.0",
        "info": {
            "title": title,
            "version": version,
        },
        "paths": {},
        "components": {
            "schemas": {}
        }
    }

    # Add ASGI app metadata if mounted
    asgi_app = app.get_asgi_app()
    if asgi_app:
        # Use OpenAPI extension field for Neutrino-specific metadata
        spec["x-neutrino-asgi"] = {
            "enabled": True,
            "app_class": f"{asgi_app.__class__.__module__}.{asgi_app.__class__.__name__}",
        }

    # Collect all schemas from routes
    schemas: dict[str, Any] = {}

    # Generate paths from routes
    for route_path in app.list_routes():
        route = app.get_route(route_path)
        openapi_path = convert_path_to_openapi(route.path)

        if openapi_path not in spec["paths"]:
            spec["paths"][openapi_path] = {}

        # Generate operation for each HTTP method
        for method in route.methods:
            operation = generate_operation(route, method)
            spec["paths"][openapi_path][method.lower()] = operation

            # Collect schemas
            if route.request_model:
                schema = pydantic_model_to_schema(route.request_model)
                if schema and "title" in schema:
                    schemas[schema["title"]] = schema

            if route.response_model:
                schema = pydantic_model_to_schema(route.response_model)
                if schema and "title" in schema:
                    schemas[schema["title"]] = schema

    # Add collected schemas to components
    if schemas:
        spec["components"]["schemas"] = schemas

    return spec
