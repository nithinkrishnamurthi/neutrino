"""
Route definitions for Neutrino orchestrated endpoints.
"""

from typing import Any, Callable


class Route:
    """Represents a registered route that will be orchestrated."""

    def __init__(
        self,
        handler: Callable[..., Any],
        path: str,
        methods: list[str] | None = None,
    ):
        self.handler = handler
        self.path = path
        self.methods = methods or ["GET"]
        self.__name__ = handler.__name__
        self.__doc__ = handler.__doc__

    def __call__(self, *args: Any, **kwargs: Any) -> Any:
        """Execute the route handler."""
        return self.handler(*args, **kwargs)

    def __repr__(self) -> str:
        methods_str = ",".join(self.methods)
        return f"<Route {self.path} [{methods_str}]>"
