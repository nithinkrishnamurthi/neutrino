"""
Neutrino App - Main application class for orchestrated routes.
"""

from typing import Any, Callable, Type

from neutrino.exceptions import ModelNotFoundError, RouteNotFoundError
from neutrino.model import Model, ModelConfig
from neutrino.route import Route


class App:
    """Main Neutrino application class for registering orchestrated routes and models."""

    def __init__(self):
        self._route_registry: dict[str, Route] = {}
        self._model_registry: dict[str, Model] = {}
        self._asgi_app: Any | None = None

    def route(
        self,
        path: str,
        methods: list[str] | None = ["GET"],
        request_model: Type[Any] | None = None,
        response_model: Type[Any] | None = None,
        summary: str | None = None,
        description: str | None = None,
        tags: list[str] | None = None,
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

        Returns:
            Decorator function that registers the route.
        """
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
            )
            self._route_registry[path] = route_obj
            return route_obj
        return decorator

    def model(
        self,
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
        """
        def decorator(cls: Type[Any]) -> Type[Any]:
            config = ModelConfig(name, cls, min_replicas, max_replicas)
            model = Model(config)
            self._model_registry[name] = model
            return cls
        return decorator

    def get_route(self, path: str) -> Route:
        """Retrieve a registered route by path.

        Args:
            path: The route path.

        Returns:
            The registered route.

        Raises:
            RouteNotFoundError: If route is not found.
        """
        try:
            return self._route_registry[path]
        except KeyError:
            raise RouteNotFoundError(f"Route '{path}' not found") from None

    def get_model(self, name: str) -> Model:
        """Retrieve a registered model by name.

        Args:
            name: The model name.

        Returns:
            The registered model.

        Raises:
            ModelNotFoundError: If model is not found.
        """
        try:
            return self._model_registry[name]
        except KeyError:
            raise ModelNotFoundError(f"Model '{name}' not found") from None

    def list_routes(self) -> list[str]:
        """List all registered route paths.

        Returns:
            List of route paths.
        """
        return list(self._route_registry.keys())

    def list_models(self) -> list[str]:
        """List all registered model names.

        Returns:
            List of model names.
        """
        return list(self._model_registry.keys())

    def mount_asgi(self, asgi_app: Any) -> None:
        """Mount an ASGI application (e.g., FastAPI, Django) as a fallback handler.

        The ASGI app will be served alongside Neutrino routes. Any route not
        registered in Neutrino will automatically fall through to the ASGI app.
        In mounted mode, the app runs in the same process via Uvicorn.
        In proxy mode, requests are forwarded to a separate service.

        Args:
            asgi_app: ASGI application instance (e.g., FastAPI() app).

        Example:
            >>> from fastapi import FastAPI
            >>> fastapi_app = FastAPI()
            >>> neutrino_app = App()
            >>> neutrino_app.mount_asgi(fastapi_app)
        """
        self._asgi_app = asgi_app

    def get_asgi_app(self) -> Any | None:
        """Get the mounted ASGI app.

        Returns:
            The ASGI app instance if mounted, None otherwise.
        """
        return self._asgi_app

    def generate_openapi(self, title: str = "Neutrino API", version: str = "1.0.0") -> dict[str, Any]:
        """Generate OpenAPI 3.0 specification from registered routes.

        Args:
            title: API title for the OpenAPI spec.
            version: API version for the OpenAPI spec.

        Returns:
            OpenAPI 3.0 specification dictionary.
        """
        from neutrino.openapi_generator import generate_openapi_spec
        return generate_openapi_spec(self, title, version)
