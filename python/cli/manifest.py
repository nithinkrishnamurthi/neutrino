"""Manifest generation for Neutrino applications."""

from datetime import datetime, timezone
from typing import Any, Dict

import yaml

from neutrino.route import Route
from neutrino.model import Model
from cli.discovery import get_class_path, get_handler_path


def generate_manifest(
    route_registry: Dict[str, Route],
    model_registry: Dict[str, Model],
    module_path: str
) -> dict[str, Any]:
    """
    Generate deployment manifest dictionary from route and model registries.

    Args:
        route_registry: Dictionary of registered routes
        model_registry: Dictionary of registered models
        module_path: The module path where routes were discovered

    Returns:
        Dictionary containing the deployment manifest
    """
    routes_dict: dict[str, dict[str, Any]] = {}
    for path, route in route_registry.items():
        routes_dict[path] = {
            "methods": route.methods,
            "handler": get_handler_path(route.handler),
        }

    models_dict: dict[str, dict[str, Any]] = {}
    for name, model in model_registry.items():
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
