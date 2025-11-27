"""
Route definitions for Neutrino orchestrated endpoints.
"""

import inspect
from typing import Any, Callable, Type, get_type_hints

try:
    from pydantic import BaseModel
    PYDANTIC_AVAILABLE = True
except ImportError:
    BaseModel = None  # type: ignore
    PYDANTIC_AVAILABLE = False


class Route:
    """Represents a registered route that will be orchestrated."""

    def __init__(
        self,
        handler: Callable[..., Any],
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
    ):
        self.handler = handler
        self.path = path
        self.methods = methods or ["GET"]
        self.request_model = request_model
        self.response_model = response_model
        self.summary = summary or handler.__name__.replace("_", " ").title()
        self.description = description or handler.__doc__
        self.tags = tags or []
        self.num_cpus = num_cpus
        self.num_gpus = num_gpus
        self.memory_gb = memory_gb
        self.__name__ = handler.__name__
        self.__doc__ = handler.__doc__

        # Auto-detect Pydantic models from type hints if not explicitly provided
        if PYDANTIC_AVAILABLE and (request_model is None or response_model is None):
            self._infer_schemas_from_type_hints()

    def _infer_schemas_from_type_hints(self) -> None:
        """Infer request/response models from function type hints."""
        try:
            type_hints = get_type_hints(self.handler)
            sig = inspect.signature(self.handler)

            # Infer response model from return type
            if self.response_model is None and "return" in type_hints:
                return_type = type_hints["return"]
                if return_type and return_type != inspect.Signature.empty:
                    # Check if it's a Pydantic model
                    if isinstance(return_type, type) and issubclass(return_type, BaseModel):
                        self.response_model = return_type

            # Infer request model from first parameter (after self/cls)
            if self.request_model is None and len(sig.parameters) > 0:
                params = list(sig.parameters.values())
                # Skip 'self' or 'cls' if present
                first_param = params[0] if params else None
                if first_param and first_param.name not in ("self", "cls"):
                    param_type = type_hints.get(first_param.name)
                    if param_type and isinstance(param_type, type) and issubclass(param_type, BaseModel):
                        self.request_model = param_type
        except Exception:
            # If type hint inference fails, continue without schemas
            pass

    def __call__(self, *args: Any, **kwargs: Any) -> Any:
        """Execute the route handler."""
        return self.handler(*args, **kwargs)

    def __repr__(self) -> str:
        methods_str = ",".join(self.methods)
        return f"<Route {self.path} [{methods_str}]>"
