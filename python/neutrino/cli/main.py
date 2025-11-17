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
def deploy(app_module: str, output: str | None, output_format: str) -> None:
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

        # Summary
        route_count = len(manifest["routes"])
        model_count = len(manifest["models"])
        click.echo(
            f"Discovered {route_count} routes and {model_count} models", err=True
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
