# Plan: Neutrino Deploy CLI

## Overview

Implement a CLI command `neutrino deploy` that introspects a Python application using Neutrino decorators and generates a YAML manifest mapping API routes to their corresponding Python handler functions. This manifest can be used for deployment configuration, service mesh routing, and documentation.

## Goals

1. Add CLI infrastructure to Neutrino Python SDK
2. Implement `neutrino deploy` command that discovers routes
3. Generate YAML output mapping routes → handler functions
4. Support common deployment patterns (stdout, file output)

## Output Format

```yaml
# neutrino-routes.yaml
version: "1"
generated_at: "2025-01-15T10:30:00Z"
app_module: "myapp.main"

routes:
  /health:
    methods: ["GET"]
    handler: "myapp.main.health_check"

  /api/users:
    methods: ["GET", "POST"]
    handler: "myapp.main.list_users"

  /api/users/{id}:
    methods: ["GET", "PUT", "DELETE"]
    handler: "myapp.main.get_user"

  /api/predict:
    methods: ["POST"]
    handler: "myapp.ml.predict_sentiment"

models:
  sentiment:
    class: "myapp.ml.SentimentAnalyzer"
    min_replicas: 1
    max_replicas: 10
```

## Implementation Plan

### Phase 1: CLI Infrastructure

**Files to create:**
- `python/neutrino/cli/__init__.py`
- `python/neutrino/cli/main.py`

**Dependencies to add:**
```toml
# pyproject.toml
dependencies = [
    "msgpack>=1.0.0",
    "click>=8.0.0",      # CLI framework
    "pyyaml>=6.0.0",     # YAML generation
]

[project.scripts]
neutrino = "neutrino.cli.main:cli"
```

**Why Click?**
- Industry standard for Python CLIs
- Excellent argument parsing
- Supports subcommands (deploy, serve, etc.)
- Good error handling and help generation

### Phase 2: App Discovery Module

**File:** `python/neutrino/cli/discovery.py`

```python
import importlib
import inspect
from neutrino import App
from neutrino.route import Route
from neutrino.model import Model


def discover_app(module_path: str) -> App:
    """
    Load a Python module and find the Neutrino App instance.

    Args:
        module_path: Dotted module path (e.g., "myapp.main")

    Returns:
        App instance found in the module

    Raises:
        ValueError: If no App instance found or multiple found
    """
    module = importlib.import_module(module_path)

    app_instances = []
    for name in dir(module):
        obj = getattr(module, name)
        if isinstance(obj, App):
            app_instances.append((name, obj))

    if len(app_instances) == 0:
        raise ValueError(f"No Neutrino App instance found in {module_path}")
    if len(app_instances) > 1:
        names = [n for n, _ in app_instances]
        raise ValueError(f"Multiple App instances found: {names}. Specify one explicitly.")

    return app_instances[0][1]


def get_handler_path(handler: callable) -> str:
    """
    Get fully qualified path for a handler function.

    Returns:
        String like "myapp.main.health_check"
    """
    module = inspect.getmodule(handler)
    if module is None:
        raise ValueError(f"Cannot determine module for {handler}")

    return f"{module.__name__}.{handler.__name__}"


def get_class_path(cls: type) -> str:
    """
    Get fully qualified path for a class.

    Returns:
        String like "myapp.ml.SentimentAnalyzer"
    """
    return f"{cls.__module__}.{cls.__qualname__}"
```

### Phase 3: YAML Generator

**File:** `python/neutrino/cli/manifest.py`

```python
from datetime import datetime, timezone
from typing import Any
import yaml
from neutrino import App
from .discovery import get_handler_path, get_class_path


def generate_manifest(app: App, module_path: str) -> dict[str, Any]:
    """
    Generate deployment manifest dictionary from App instance.
    """
    routes_dict = {}
    for path in app.list_routes():
        route = app.get_route(path)
        routes_dict[path] = {
            "methods": route.methods,
            "handler": get_handler_path(route.handler),
        }

    models_dict = {}
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
    """
    return yaml.dump(
        manifest,
        default_flow_style=False,
        sort_keys=False,
        allow_unicode=True,
    )
```

### Phase 4: CLI Command Implementation

**File:** `python/neutrino/cli/main.py`

```python
import click
import sys
from pathlib import Path
from .discovery import discover_app
from .manifest import generate_manifest, manifest_to_yaml


@click.group()
@click.version_option(version="0.1.0", prog_name="neutrino")
def cli():
    """Neutrino - High-performance distributed orchestration framework."""
    pass


@cli.command()
@click.argument("app_module", required=True)
@click.option(
    "--output", "-o",
    type=click.Path(dir_okay=False, writable=True),
    help="Output file path. Defaults to stdout if not specified.",
)
@click.option(
    "--format", "-f",
    type=click.Choice(["yaml", "json"]),
    default="yaml",
    help="Output format (default: yaml)",
)
def deploy(app_module: str, output: str | None, format: str):
    """
    Generate deployment manifest for a Neutrino application.

    APP_MODULE is the Python module path containing your App instance
    (e.g., 'myapp.main' or 'myapp:app').

    Examples:

        neutrino deploy myapp.main

        neutrino deploy myapp.main -o neutrino-routes.yaml

        neutrino deploy myapp.main --format json
    """
    # Handle module:variable syntax
    if ":" in app_module:
        module_path, var_name = app_module.split(":", 1)
        # TODO: Support explicit variable name
        click.echo(f"Error: Explicit variable syntax not yet supported", err=True)
        sys.exit(1)
    else:
        module_path = app_module

    # Add current directory to path for local imports
    sys.path.insert(0, str(Path.cwd()))

    try:
        # Discover app
        click.echo(f"Discovering routes in {module_path}...", err=True)
        app = discover_app(module_path)

        # Generate manifest
        manifest = generate_manifest(app, module_path)

        # Format output
        if format == "yaml":
            content = manifest_to_yaml(manifest)
        else:  # json
            import json
            content = json.dumps(manifest, indent=2, default=str)

        # Write output
        if output:
            Path(output).write_text(content)
            click.echo(f"Manifest written to {output}", err=True)
        else:
            click.echo(content)

        # Summary
        route_count = len(manifest["routes"])
        model_count = len(manifest["models"])
        click.echo(
            f"Discovered {route_count} routes and {model_count} models",
            err=True
        )

    except ImportError as e:
        click.echo(f"Error: Could not import module '{module_path}'", err=True)
        click.echo(f"Details: {e}", err=True)
        sys.exit(1)
    except ValueError as e:
        click.echo(f"Error: {e}", err=True)
        sys.exit(1)
    except Exception as e:
        click.echo(f"Unexpected error: {e}", err=True)
        sys.exit(1)


if __name__ == "__main__":
    cli()
```

### Phase 5: Entry Point Configuration

**Update:** `python/pyproject.toml`

```toml
[project]
name = "neutrino"
version = "0.1.0"
description = "High-performance distributed orchestration and model serving framework"
readme = "README.md"
requires-python = ">=3.11"
dependencies = [
    "msgpack>=1.0.0",
    "click>=8.0.0",
    "pyyaml>=6.0.0",
]

[project.scripts]
neutrino = "neutrino.cli.main:cli"
```

## Directory Structure After Implementation

```
python/neutrino/
├── __init__.py
├── app.py
├── route.py
├── model.py
├── exceptions.py
├── cli/                    # NEW
│   ├── __init__.py
│   ├── main.py            # CLI entry point
│   ├── discovery.py       # App introspection
│   └── manifest.py        # YAML generation
└── internal/
    └── worker/
        ├── main.py
        └── protocol.py
```

## Usage Examples

### Basic Usage

```bash
# Generate manifest to stdout
$ neutrino deploy myapp.main
version: '1'
generated_at: '2025-01-15T10:30:00+00:00'
app_module: myapp.main
routes:
  /health:
    methods: ['GET']
    handler: myapp.main.health_check
  /api/users:
    methods: ['GET', 'POST']
    handler: myapp.main.list_users
models: {}
```

### Save to File

```bash
$ neutrino deploy myapp.main -o neutrino-routes.yaml
Discovering routes in myapp.main...
Manifest written to neutrino-routes.yaml
Discovered 5 routes and 2 models
```

### JSON Output

```bash
$ neutrino deploy myapp.main --format json -o routes.json
```

### Example Application

```python
# myapp/main.py
from neutrino import App

app = App()

@app.route("/health", methods=["GET"])
def health_check():
    return {"status": "ok"}

@app.route("/api/users", methods=["GET", "POST"])
def list_users():
    return []

@app.route("/api/users/{id}", methods=["GET", "PUT", "DELETE"])
def get_user(id: str):
    return {"id": id}

@app.model(name="sentiment", min_replicas=2, max_replicas=20)
class SentimentAnalyzer:
    def load(self):
        pass

    def predict(self, text: str):
        return {"sentiment": "positive"}
```

## Testing Strategy

### Unit Tests

**File:** `python/tests/test_cli.py`

```python
import pytest
from click.testing import CliRunner
from neutrino.cli.main import cli
from neutrino.cli.discovery import discover_app, get_handler_path


class TestDiscovery:
    def test_discover_app_finds_single_instance(self):
        # Create test module with single App
        pass

    def test_discover_app_raises_on_multiple(self):
        pass

    def test_discover_app_raises_on_none(self):
        pass

    def test_get_handler_path(self):
        def my_func():
            pass
        path = get_handler_path(my_func)
        assert path.endswith(".my_func")


class TestManifest:
    def test_generate_manifest_with_routes(self):
        pass

    def test_generate_manifest_with_models(self):
        pass

    def test_manifest_to_yaml_format(self):
        pass


class TestCLI:
    def test_deploy_command_basic(self):
        runner = CliRunner()
        result = runner.invoke(cli, ["deploy", "test_app"])
        # Assert output format

    def test_deploy_command_file_output(self):
        pass

    def test_deploy_command_json_format(self):
        pass

    def test_deploy_command_missing_module(self):
        runner = CliRunner()
        result = runner.invoke(cli, ["deploy", "nonexistent.module"])
        assert result.exit_code != 0
```

### Integration Tests

Create a sample app and verify end-to-end generation:

```bash
$ cd examples/
$ neutrino deploy sample_app.main -o manifest.yaml
$ diff manifest.yaml expected_manifest.yaml
```

## Future Enhancements

### Phase 2 Features (v0.2)

1. **Kubernetes YAML Generation**
   ```bash
   $ neutrino deploy myapp.main --kubernetes -o k8s/
   ```
   Generates:
   - `deployment.yaml` - K8s Deployment
   - `service.yaml` - K8s Service
   - `hpa.yaml` - HorizontalPodAutoscaler

2. **Validation**
   ```bash
   $ neutrino deploy myapp.main --validate
   ```
   Check for:
   - Duplicate routes
   - Invalid HTTP methods
   - Missing model configurations

3. **Diff Mode**
   ```bash
   $ neutrino deploy myapp.main --diff current-manifest.yaml
   ```
   Show changes between current and generated manifest

4. **Environment Variables**
   ```bash
   $ NEUTRINO_APP=myapp.main neutrino deploy
   ```

### Phase 3 Features (v0.3)

1. **OpenAPI Generation**
   ```bash
   $ neutrino deploy myapp.main --openapi
   ```

2. **Service Mesh Integration**
   - Istio VirtualService generation
   - Envoy route configuration

3. **Multi-App Support**
   ```bash
   $ neutrino deploy myapp.main myapp.admin --merge
   ```

## Implementation Order

1. **Add dependencies** - Update pyproject.toml with click and pyyaml
2. **Create CLI skeleton** - Basic click group and deploy command
3. **Implement discovery** - Module loading and App detection
4. **Implement manifest generation** - Route/model introspection
5. **Wire up entry point** - Add [project.scripts] configuration
6. **Write tests** - Unit and integration tests
7. **Add examples** - Sample app demonstrating usage
8. **Documentation** - Update README with CLI usage

## Estimated Effort

- Phase 1 (CLI Infrastructure): 2 hours
- Phase 2 (App Discovery): 2 hours
- Phase 3 (YAML Generator): 1 hour
- Phase 4 (CLI Command): 2 hours
- Phase 5 (Entry Point): 0.5 hours
- Testing: 3 hours
- Documentation: 1 hour

**Total: ~11.5 hours**

## Success Criteria

1. `pip install .` in python/ directory makes `neutrino` command available
2. `neutrino deploy myapp.main` outputs valid YAML mapping routes → handlers
3. Output includes both routes and models
4. File output option works (`-o file.yaml`)
5. JSON format option works (`--format json`)
6. Helpful error messages for common mistakes
7. All tests pass
8. Example app demonstrates usage
