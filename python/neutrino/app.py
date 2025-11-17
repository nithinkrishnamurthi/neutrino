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

    def route(
        self,
        path: str,
        methods: list[str] | None = None,
    ) -> Callable[[Callable[..., Any]], Route]:
        """Decorator to register a function as an orchestrated route.

        Args:
            path: The route path (e.g., "/analyze", "/process").
            methods: HTTP methods for the route. Defaults to ["GET"].

        Returns:
            Decorator function that registers the route.
        """
        def decorator(func: Callable[..., Any]) -> Route:
            route_obj = Route(func, path, methods)
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
