# Neutrino - High-performance distributed orchestration framework
__version__ = "0.1.0"

from typing import Any, Callable, Type
from neutrino.exceptions import (
    ModelError,
    ModelNotFoundError,
    NeutrinoError,
    ProtocolError,
    RouteError,
    RouteNotFoundError,
    WorkerError,
)
from neutrino.model import Model, ModelConfig
from neutrino.route import Route

# Global registries for routes and models
_global_route_registry: dict[str, Route] = {}
_global_model_registry: dict[str, Model] = {}
_global_asgi_app: Any | None = None


def route(
    path: str,
    methods: list[str] | None = None,
    request_model: Type[Any] | None = None,
    response_model: Type[Any] | None = None,
    summary: str | None = None,
    description: str | None = None,
    tags: list[str] | None = None,
    num_cpus: float = 1.0,
    num_gpus: float = 0.0,
    memory_gb: float = 1.0,
) -> Callable[[Callable[..., Any]], Route]:
    """Decorator to register a function as an orchestrated route.

    Args:
        path: The route path (e.g., "/analyze", "/process").
        methods: HTTP methods for the route. Defaults to ["GET"].
        request_model: Optional Pydantic model for request validation.
        response_model: Optional Pydantic model for response serialization.
        summary: Optional short summary for OpenAPI documentation.
        description: Optional detailed description for OpenAPI documentation.
        tags: Optional list of tags for grouping routes in OpenAPI docs.
        num_cpus: CPUs required (logical cores, can be fractional). Defaults to 1.0.
        num_gpus: GPUs required (devices, can be fractional). Defaults to 0.0.
        memory_gb: Memory required in GB. Defaults to 1.0.

    Returns:
        Decorator function that registers the route.

    Example:
        >>> @route("/api/users", methods=["GET"])
        ... def list_users():
        ...     return {"users": [...]}

        >>> @route("/inference", methods=["POST"], num_cpus=2, num_gpus=1)
        ... def run_inference(data: dict):
        ...     return model.predict(data)
    """
    if methods is None:
        methods = ["GET"]

    def decorator(func: Callable[..., Any]) -> Route:
        route_obj = Route(
            func,
            path,
            methods,
            request_model,
            response_model,
            summary,
            description,
            tags,
            num_cpus,
            num_gpus,
            memory_gb,
        )
        _global_route_registry[path] = route_obj
        return route_obj

    return decorator


def model(
    name: str,
    min_replicas: int = 1,
    max_replicas: int = 10,
) -> Callable[[Type[Any]], Type[Any]]:
    """Decorator to register a class as a model.

    Args:
        name: Name for the model deployment.
        min_replicas: Minimum number of model replicas.
        max_replicas: Maximum number of model replicas.

    Returns:
        Decorator function that registers the model.

    Example:
        >>> @model(name="sentiment", min_replicas=1, max_replicas=10)
        ... class SentimentAnalyzer:
        ...     def load(self):
        ...         self.model = pipeline("sentiment-analysis")
        ...
        ...     def predict(self, text: str):
        ...         return self.model(text)
    """
    def decorator(cls: Type[Any]) -> Type[Any]:
        config = ModelConfig(name, cls, min_replicas, max_replicas)
        model_obj = Model(config)
        _global_model_registry[name] = model_obj
        return cls

    return decorator


def mount_asgi(asgi_app: Any) -> None:
    """Mount an ASGI application (e.g., FastAPI, Django) as a fallback handler.

    The ASGI app will be served alongside Neutrino routes. Any route not
    registered in Neutrino will automatically fall through to the ASGI app.
    In mounted mode, the app runs in the same process via Uvicorn.
    In proxy mode, requests are forwarded to a separate service.

    Args:
        asgi_app: ASGI application instance (e.g., FastAPI() app).

    Example:
        >>> from fastapi import FastAPI
        >>> from neutrino import route, mount_asgi
        >>>
        >>> fastapi_app = FastAPI()
        >>>
        >>> @fastapi_app.get("/health")
        >>> def health():
        ...     return {"status": "ok"}
        >>>
        >>> @route("/neutrino/process", methods=["POST"])
        >>> def process_task(data: dict):
        ...     return {"result": ...}
        >>>
        >>> mount_asgi(fastapi_app)
    """
    global _global_asgi_app
    _global_asgi_app = asgi_app


def get_route(path: str) -> Route:
    """Retrieve a registered route by path.

    Args:
        path: The route path.

    Returns:
        The registered route.

    Raises:
        RouteNotFoundError: If route is not found.
    """
    try:
        return _global_route_registry[path]
    except KeyError:
        raise RouteNotFoundError(f"Route '{path}' not found") from None


def get_model(name: str) -> Model:
    """Retrieve a registered model by name.

    Args:
        name: The model name.

    Returns:
        The registered model.

    Raises:
        ModelNotFoundError: If model is not found.
    """
    try:
        return _global_model_registry[name]
    except KeyError:
        raise ModelNotFoundError(f"Model '{name}' not found") from None


def list_routes() -> list[str]:
    """List all registered route paths.

    Returns:
        List of route paths.
    """
    return list(_global_route_registry.keys())


def list_models() -> list[str]:
    """List all registered model names.

    Returns:
        List of model names.
    """
    return list(_global_model_registry.keys())


def get_asgi_app() -> Any | None:
    """Get the mounted ASGI app.

    Returns:
        The ASGI app instance if mounted, None otherwise.
    """
    return _global_asgi_app


def generate_openapi(title: str = "Neutrino API", version: str = "1.0.0") -> dict[str, Any]:
    """Generate OpenAPI 3.0 specification from registered routes.

    Args:
        title: API title for the OpenAPI spec.
        version: API version for the OpenAPI spec.

    Returns:
        OpenAPI 3.0 specification dictionary.
    """
    from neutrino.openapi_generator import generate_openapi_spec
    return generate_openapi_spec(_global_route_registry, _global_model_registry, title, version, _global_asgi_app)


__all__ = [
    # Core
    "__version__",
    # Decorators
    "route",
    "model",
    # ASGI integration
    "mount_asgi",
    # Route management
    "Route",
    "get_route",
    "list_routes",
    # Model management
    "Model",
    "ModelConfig",
    "get_model",
    "list_models",
    # ASGI app access
    "get_asgi_app",
    # OpenAPI generation
    "generate_openapi",
    # Exceptions
    "NeutrinoError",
    "RouteError",
    "RouteNotFoundError",
    "ModelError",
    "ModelNotFoundError",
    "WorkerError",
    "ProtocolError",
]
