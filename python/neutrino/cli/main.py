"""Main CLI entry point for Neutrino."""

import json
import sys
from pathlib import Path

import click

from neutrino.cli.discovery import discover_app
from neutrino.cli.manifest import generate_manifest, manifest_to_yaml

import sys

@click.group()
@click.version_option(version="0.1.0", prog_name="neutrino")
def cli() -> None:
    """Neutrino - High-performance distributed orchestration framework."""
    pass


@cli.command()
@click.argument("app_module", required=True)
@click.option(
    "--output",
    "-o",
    type=click.Path(dir_okay=False, writable=True),
    help="Output file path. Defaults to stdout if not specified.",
)
@click.option(
    "--format",
    "-f",
    "output_format",
    type=click.Choice(["yaml", "json"]),
    default="yaml",
    help="Output format (default: yaml)",
)
@click.option(
    "--openapi",
    is_flag=True,
    default=False,
    help="Also generate openapi.json file for Rust router",
)
def deploy(app_module: str, output: str | None, output_format: str, openapi: bool) -> None:
    """
    Generate deployment manifest for a Neutrino application.

    APP_MODULE is the Python module path containing your App instance
    (e.g., 'myapp.main' or 'myapp:app').

    Examples:

        neutrino deploy myapp.main

        neutrino deploy myapp.main -o neutrino-routes.yaml

        neutrino deploy myapp.main --format json

        neutrino deploy myapp.main --openapi
    """


    # Handle module:variable syntax

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
        if output_format == "yaml":
            content = manifest_to_yaml(manifest)
        else:  # json
            content = json.dumps(manifest, indent=2, default=str)

        # Write output
        if output:
            Path(output).write_text(content)
            click.echo(f"Manifest written to {output}", err=True)
        else:
            click.echo(content)

        # Generate OpenAPI spec if requested
        if openapi:
            openapi_spec = app.generate_openapi()
            openapi_path = Path("openapi.json")
            openapi_path.write_text(json.dumps(openapi_spec, indent=2))
            click.echo(f"OpenAPI spec written to {openapi_path}", err=True)

            # Check if ASGI app is mounted and generate Uvicorn config
            asgi_info = app.get_asgi_app()
            if asgi_info:
                path_prefix, asgi_app = asgi_info
                asgi_module = f"{asgi_app.__class__.__module__}"

                # Generate uvicorn startup script
                uvicorn_script = f'''#!/usr/bin/env python3
"""
Auto-generated Uvicorn startup script for ASGI app.
This script is used by Neutrino to run the ASGI app in mounted mode.
"""

import sys
from pathlib import Path

# Add current directory to path for imports
sys.path.insert(0, str(Path.cwd()))

# Import the app
from {module_path.rsplit(":", 1)[0]} import *

# Get the ASGI app instance
asgi_info = app.get_asgi_app()
if asgi_info:
    _, asgi_application = asgi_info
else:
    raise RuntimeError("No ASGI app found in Neutrino app")

# This is what Uvicorn will look for
app = asgi_application
'''

                uvicorn_script_path = Path("uvicorn_app.py")
                uvicorn_script_path.write_text(uvicorn_script)
                click.echo(f"Uvicorn script written to {uvicorn_script_path}", err=True)
                click.echo(f"  ASGI app mounted at: {path_prefix}", err=True)

        # Summary
        route_count = len(manifest["routes"])
        model_count = len(manifest["models"])
        asgi_status = "with ASGI integration" if app.get_asgi_app() else ""
        click.echo(
            f"Discovered {route_count} routes and {model_count} models {asgi_status}", err=True
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
