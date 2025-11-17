"""Manifest generation for Neutrino applications."""

from datetime import datetime, timezone
from typing import Any

import yaml

from neutrino import App
from neutrino.cli.discovery import get_class_path, get_handler_path


def generate_manifest(app: App, module_path: str) -> dict[str, Any]:
    """
    Generate deployment manifest dictionary from App instance.

    Args:
        app: The Neutrino App instance
        module_path: The module path where the app was found

    Returns:
        Dictionary containing the deployment manifest
    """
    routes_dict: dict[str, dict[str, Any]] = {}
    for path in app.list_routes():
        route = app.get_route(path)
        routes_dict[path] = {
            "methods": route.methods,
            "handler": get_handler_path(route.handler),
        }

    models_dict: dict[str, dict[str, Any]] = {}
    for name in app.list_models():
        model = app.get_model(name)
        models_dict[name] = {
            "class": get_class_path(model.config.cls),
            "min_replicas": model.config.min_replicas,
            "max_replicas": model.config.max_replicas,
        }

    return {
        "version": "1",
        "generated_at": datetime.now(timezone.utc).isoformat(),
        "app_module": module_path,
        "routes": routes_dict,
        "models": models_dict,
    }


def manifest_to_yaml(manifest: dict[str, Any]) -> str:
    """
    Convert manifest dictionary to YAML string.

    Args:
        manifest: The manifest dictionary

    Returns:
        YAML formatted string
    """
    return yaml.dump(
        manifest,
        default_flow_style=False,
        sort_keys=False,
        allow_unicode=True,
    )
