"""App discovery module for introspecting Neutrino applications."""

import importlib
import inspect
import sys
import os
import importlib.util
from typing import Callable


def import_module(module_str: str):
    """
    Import a Python module from either a file path or dotted module path.

    Uvicorn's logic:
    - if module_str ends with ".py", treat as file path
    - otherwise treat as importable dotted module path

    Args:
        module_str: File path or module path to import

    Returns:
        The imported module
    """

    # Case 1: import from file path
    if module_str.endswith(".py") or os.path.sep in module_str:
        # Normalize path
        path = os.path.abspath(module_str)
        module_name = os.path.splitext(os.path.basename(path))[0]

        spec = importlib.util.spec_from_file_location(module_name, path)
        module = importlib.util.module_from_spec(spec)

        sys.modules[module_name] = module
        spec.loader.exec_module(module)
        return module

    # Case 2: import from dotted path
    return importlib.import_module(module_str)


def get_handler_path(handler: Callable) -> str:  # type: ignore[type-arg]
    """
    Get fully qualified path for a handler function.

    Args:
        handler: The handler function

    Returns:
        String like "myapp.main.health_check"

    Raises:
        ValueError: If module cannot be determined
    """
    module = inspect.getmodule(handler)
    if module is None:
        raise ValueError(f"Cannot determine module for {handler}")

    return f"{module.__name__}.{handler.__name__}"


def get_class_path(cls: type) -> str:
    """
    Get fully qualified path for a class.

    Args:
        cls: The class type

    Returns:
        String like "myapp.ml.SentimentAnalyzer"
    """
    return f"{cls.__module__}.{cls.__qualname__}"
