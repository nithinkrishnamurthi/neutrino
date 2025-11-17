"""App discovery module for introspecting Neutrino applications."""

import importlib
import inspect
from typing import Callable

from neutrino import App
import sys
import os
import importlib.util

def import_module(module_str: str):
    """
    Uvicorn's logic:
    - if module_str ends with ".py", treat as file path
    - otherwise treat as importable dotted module path
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


def discover_app(target: str) -> App:
    """
    Load a Python module and find the Neutrino App instance.

    target: str like
      - "test_cli.app"
      - "test_cli.app:app"
      - "test_cli/app.py:app"
    """

    # Split module path vs attribute
    if ":" in target:
        module_str, attr_name = target.split(":", 1)
    else:
        module_str, attr_name = target, None

    module = import_module(module_str)

    if attr_name:
        obj = getattr(module, attr_name, None)
        if obj is None:
            raise ValueError(f"Object '{attr_name}' not found in module '{module_str}'")
        if not isinstance(obj, App):
            raise TypeError(f"Object '{attr_name}' in module '{module_str}' is not a Neutrino App")
        return obj

    # fallback: auto-discover a single App in module
    app_instances: list[tuple[str, App]] = []
    for name in dir(module):
        obj = getattr(module, name)
        if isinstance(obj, App):
            app_instances.append((name, obj))

    if len(app_instances) == 0:
        raise ValueError(f"No Neutrino App instance found in {module_str}")
    if len(app_instances) > 1:
        names = [n for n, _ in app_instances]
        raise ValueError(f"Multiple App instances found: {names}. Specify one explicitly.")

    return app_instances[0][1]


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
