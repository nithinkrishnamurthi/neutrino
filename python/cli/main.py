"""Main CLI entry point for Neutrino."""

import json
import os
import subprocess
import sys
from pathlib import Path

# Add project root to sys.path for cli module imports
project_root = Path(__file__).parent.parent
if str(project_root) not in sys.path:
    sys.path.insert(0, str(project_root))

import click

from cli.discovery import discover_app
from cli.manifest import generate_manifest, manifest_to_yaml

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
            asgi_app = app.get_asgi_app()
            if asgi_app:
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
asgi_application = app.get_asgi_app()
if asgi_application is None:
    raise RuntimeError("No ASGI app found in Neutrino app")

# This is what Uvicorn will look for
app = asgi_application
'''

                uvicorn_script_path = Path("uvicorn_app.py")
                uvicorn_script_path.write_text(uvicorn_script)
                click.echo(f"Uvicorn script written to {uvicorn_script_path}", err=True)
                click.echo(f"  ASGI app will handle unmatched routes", err=True)

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


@cli.command()
@click.option(
    "--app-module",
    "-a",
    default="examples.fastapi_integration",
    help="Python module containing your App instance (default: examples.fastapi_integration)",
)
@click.option(
    "--image-name",
    "-i",
    default="neutrino:latest",
    help="Docker image name (default: neutrino:latest)",
)
@click.option(
    "--namespace",
    "-n",
    default="default",
    help="Kubernetes namespace (default: default)",
)
@click.option(
    "--skip-docker",
    is_flag=True,
    default=False,
    help="Skip Docker build and import steps",
)
def up(app_module: str, image_name: str, namespace: str, skip_docker: bool) -> None:
    """
    Deploy Neutrino application to k3s cluster.

    This command will:
    1. Check prerequisites (kubectl, docker, k3s)
    2. Generate OpenAPI spec and Uvicorn startup script
    3. Build Docker image
    4. Import image to k3s
    5. Deploy to k3s cluster

    Examples:

        neutrino up

        neutrino up --app-module myapp.main

        neutrino up --namespace production

        neutrino up --skip-docker  # Use existing image
    """

    # Colors for output
    RED = '\033[0;31m'
    GREEN = '\033[0;32m'
    YELLOW = '\033[1;33m'
    NC = '\033[0m'  # No Color

    def echo_color(msg: str, color: str = NC) -> None:
        click.echo(f"{color}{msg}{NC}", err=True)

    def run_command(cmd: str, description: str, check: bool = True, shell: bool = True) -> subprocess.CompletedProcess:
        """Run a shell command and handle errors."""
        echo_color(f"Running: {description}", YELLOW)
        result = subprocess.run(cmd, shell=shell, capture_output=True, text=True)
        if check and result.returncode != 0:
            echo_color(f"Error: {description} failed", RED)
            echo_color(f"Command: {cmd}", RED)
            echo_color(f"Output: {result.stderr}", RED)
            sys.exit(1)
        return result

    echo_color("=== Neutrino k3s Deployment ===", GREEN)
    echo_color("")

    # Step 1: Check prerequisites
    echo_color("[1/6] Checking prerequisites...", YELLOW)

    # Check kubectl
    if subprocess.run("command -v kubectl", shell=True, capture_output=True).returncode != 0:
        echo_color("Error: kubectl not found. Please install kubectl.", RED)
        sys.exit(1)

    # Check docker (only if not skipping)
    if not skip_docker:
        if subprocess.run("command -v docker", shell=True, capture_output=True).returncode != 0:
            echo_color("Error: docker not found. Please install docker.", RED)
            sys.exit(1)

    # Setup kubeconfig for k3s if needed
    result = subprocess.run("kubectl cluster-info", shell=True, capture_output=True)
    if result.returncode != 0:
        echo_color("kubectl not configured. Setting up kubeconfig for k3s...", YELLOW)

        # Check if k3s is running
        result = subprocess.run("systemctl is-active --quiet k3s", shell=True)
        if result.returncode != 0:
            echo_color("Error: k3s service is not running. Start it with: sudo systemctl start k3s", RED)
            sys.exit(1)

        # Copy k3s config
        k3s_config = Path("/etc/rancher/k3s/k3s.yaml")
        if k3s_config.exists():
            kube_dir = Path.home() / ".kube"
            kube_dir.mkdir(exist_ok=True)
            subprocess.run(f"sudo cp {k3s_config} {kube_dir}/config", shell=True, check=True)
            subprocess.run(f"sudo chown {os.getenv('USER')}:{os.getenv('USER')} {kube_dir}/config", shell=True, check=True)
            subprocess.run(f"chmod 600 {kube_dir}/config", shell=True, check=True)
            os.environ["KUBECONFIG"] = str(kube_dir / "config")
            echo_color("✓ Kubeconfig configured", GREEN)

            # Verify connection
            if subprocess.run("kubectl cluster-info", shell=True, capture_output=True).returncode != 0:
                echo_color("Error: Still cannot connect to k3s. Try: sudo k3s kubectl get nodes", RED)
                sys.exit(1)
        else:
            echo_color(f"Error: k3s config not found at {k3s_config}", RED)
            sys.exit(1)

    echo_color("✓ Prerequisites check passed", GREEN)
    echo_color("")

    # Step 2: Generate OpenAPI spec and Uvicorn script
    echo_color("[2/6] Generating OpenAPI spec and Uvicorn script...", YELLOW)

    # Get project root
    project_root = Path.cwd()

    # Activate virtual environment if it exists
    venv_paths = [project_root / ".venv", project_root / "python" / ".venv"]
    for venv_path in venv_paths:
        if venv_path.exists():
            activate_script = venv_path / "bin" / "activate"
            # Note: We can't source in subprocess, so we'll use python directly
            break

    # Run neutrino deploy command
    result = subprocess.run(
        f"{sys.executable} -m cli.main deploy {app_module} --openapi",
        shell=True,
        capture_output=True,
        text=True
    )

    if result.returncode != 0:
        echo_color(f"Error: Failed to generate deployment files", RED)
        echo_color(f"Output: {result.stderr}", RED)
        sys.exit(1)

    # Check for generated files
    if not Path("openapi.json").exists():
        echo_color("Error: Failed to generate openapi.json", RED)
        sys.exit(1)

    if not Path("uvicorn_app.py").exists():
        echo_color("Error: Failed to generate uvicorn_app.py", RED)
        sys.exit(1)

    echo_color("✓ Generated openapi.json and uvicorn_app.py", GREEN)
    echo_color("")

    if not skip_docker:
        # Step 3: Build Docker images
        echo_color("[3/7] Building Docker images...", YELLOW)

        # Build main Neutrino image
        run_command(f"docker build -t {image_name} .", "Docker build (neutrino)")

        # Build dashboard image
        dashboard_image = "neutrino-dashboard:latest"
        run_command(f"docker build -t {dashboard_image} -f dashboard/Dockerfile dashboard/", "Docker build (dashboard)")

        # Build gateway image
        gateway_image = "neutrino-gateway:latest"
        run_command(f"docker build -t {gateway_image} -f gateway/Dockerfile .", "Docker build (gateway)")

        echo_color(f"✓ Docker images built", GREEN)
        echo_color("")

        # Step 4: Import images to k3s
        echo_color("[4/7] Importing images to k3s...", YELLOW)

        # Save and import main Neutrino image
        run_command(f"docker save {image_name} -o /tmp/neutrino-image.tar", "Docker save (neutrino)")

        if subprocess.run("command -v k3s", shell=True, capture_output=True).returncode == 0:
            run_command("sudo k3s ctr images import /tmp/neutrino-image.tar", "k3s import (neutrino)")
        elif subprocess.run("command -v crictl", shell=True, capture_output=True).returncode == 0:
            run_command("sudo crictl pull docker-archive:///tmp/neutrino-image.tar", "crictl import (neutrino)", check=False)
        else:
            echo_color("Warning: Could not import neutrino to k3s. Image may need to be pulled from registry.", YELLOW)

        subprocess.run("rm -f /tmp/neutrino-image.tar", shell=True)

        # Save and import dashboard image
        run_command(f"docker save {dashboard_image} -o /tmp/neutrino-dashboard-image.tar", "Docker save (dashboard)")

        if subprocess.run("command -v k3s", shell=True, capture_output=True).returncode == 0:
            run_command("sudo k3s ctr images import /tmp/neutrino-dashboard-image.tar", "k3s import (dashboard)")
        elif subprocess.run("command -v crictl", shell=True, capture_output=True).returncode == 0:
            run_command("sudo crictl pull docker-archive:///tmp/neutrino-dashboard-image.tar", "crictl import (dashboard)", check=False)
        else:
            echo_color("Warning: Could not import dashboard to k3s. Image may need to be pulled from registry.", YELLOW)

        subprocess.run("rm -f /tmp/neutrino-dashboard-image.tar", shell=True)

        # Save and import gateway image
        run_command(f"docker save {gateway_image} -o /tmp/neutrino-gateway-image.tar", "Docker save (gateway)")

        if subprocess.run("command -v k3s", shell=True, capture_output=True).returncode == 0:
            run_command("sudo k3s ctr images import /tmp/neutrino-gateway-image.tar", "k3s import (gateway)")
        elif subprocess.run("command -v crictl", shell=True, capture_output=True).returncode == 0:
            run_command("sudo crictl pull docker-archive:///tmp/neutrino-gateway-image.tar", "crictl import (gateway)", check=False)
        else:
            echo_color("Warning: Could not import gateway to k3s. Image may need to be pulled from registry.", YELLOW)

        subprocess.run("rm -f /tmp/neutrino-gateway-image.tar", shell=True)

        echo_color("✓ Images imported to k3s", GREEN)
        echo_color("")
    else:
        echo_color("[3/7] Skipping Docker build (--skip-docker)", YELLOW)
        echo_color("[4/7] Skipping image import (--skip-docker)", YELLOW)
        echo_color("")

    # Step 5: Create Kubernetes resources
    echo_color("[5/7] Creating Kubernetes resources...", YELLOW)

    # Create ConfigMap from generated files
    run_command(
        f"kubectl create configmap neutrino-app-code "
        f"--from-file=openapi.json=openapi.json "
        f"--from-file=uvicorn_app.py=uvicorn_app.py "
        f"--namespace={namespace} "
        f"--dry-run=client -o yaml | kubectl apply -f -",
        "Create ConfigMap"
    )

    # Apply all k8s manifests for main application
    run_command(f"kubectl apply -f k8s/configmap.yaml --namespace={namespace}", "Apply configmap.yaml")
    run_command(f"kubectl apply -f k8s/deployment.yaml --namespace={namespace}", "Apply deployment.yaml")
    run_command(f"kubectl apply -f k8s/service.yaml --namespace={namespace}", "Apply service.yaml")

    # Apply dashboard manifests
    run_command(f"kubectl apply -f k8s/dashboard-deployment.yaml --namespace={namespace}", "Apply dashboard-deployment.yaml")
    run_command(f"kubectl apply -f k8s/dashboard-service.yaml --namespace={namespace}", "Apply dashboard-service.yaml")

    # Apply gateway manifests
    run_command(f"kubectl apply -f k8s/gateway-deployment.yaml --namespace={namespace}", "Apply gateway-deployment.yaml")
    run_command(f"kubectl apply -f k8s/gateway-service.yaml --namespace={namespace}", "Apply gateway-service.yaml")

    echo_color("✓ Kubernetes resources created", GREEN)
    echo_color("")

    # Step 6: Wait for deployments
    echo_color("[6/7] Waiting for deployments to be ready...", YELLOW)

    # Wait for main deployment
    result = run_command(
        f"kubectl rollout status deployment/neutrino --namespace={namespace} --timeout=120s",
        "Wait for neutrino rollout",
        check=False
    )

    # Wait for dashboard deployment
    dashboard_result = run_command(
        f"kubectl rollout status deployment/neutrino-dashboard --namespace={namespace} --timeout=120s",
        "Wait for dashboard rollout",
        check=False
    )

    # Wait for gateway deployment
    gateway_result = run_command(
        f"kubectl rollout status deployment/neutrino-gateway --namespace={namespace} --timeout=120s",
        "Wait for gateway rollout",
        check=False
    )

    if result.returncode == 0 and dashboard_result.returncode == 0 and gateway_result.returncode == 0:
        echo_color("✓ Deployments ready", GREEN)
    else:
        echo_color("Warning: Deployment may not be ready yet. Check with: kubectl get pods -n {namespace}", YELLOW)

    echo_color("")

    # Step 7: Setup port forwarding
    echo_color("[7/7] Setting up port forwarding...", YELLOW)

    # Start port forwarding in background
    echo_color("Starting port forward for Gateway (8080)...", NC)
    subprocess.Popen(
        f"kubectl port-forward -n {namespace} service/neutrino-gateway 8080:8080",
        shell=True,
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL
    )

    echo_color("Starting port forward for Dashboard (8081)...", NC)
    subprocess.Popen(
        f"kubectl port-forward -n {namespace} service/neutrino-dashboard 8081:8081",
        shell=True,
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL
    )

    # Give port forwards a moment to establish
    import time
    time.sleep(2)

    echo_color("✓ Port forwarding active", GREEN)
    echo_color("")

    echo_color("=== Deployment Complete ===", GREEN)
    echo_color("")
    echo_color("Neutrino is available at:", GREEN)
    echo_color("  Gateway (API): http://localhost:8080 (includes ASGI proxy + DB logging)", NC)
    echo_color("  Dashboard:     http://localhost:8081", NC)
    echo_color("")
    echo_color("Test endpoints:", NC)
    echo_color("  Health check:  curl http://localhost:8080/health", NC)
    echo_color("  Dashboard:     curl http://localhost:8081/health", NC)
    echo_color("  View jobs:     open http://localhost:8081", NC)
    echo_color("")
    echo_color("View logs:", NC)
    echo_color(f"  Gateway:   kubectl logs -f deployment/neutrino-gateway --namespace={namespace}", NC)
    echo_color(f"  Neutrino:  kubectl logs -f deployment/neutrino --namespace={namespace}", NC)
    echo_color(f"  Dashboard: kubectl logs -f deployment/neutrino-dashboard --namespace={namespace}", NC)
    echo_color("")
    echo_color("View pods:", NC)
    echo_color(f"  kubectl get pods --namespace={namespace}", NC)
    echo_color("")
    echo_color("Port forwarding is running in the background.", YELLOW)
    echo_color("To stop port forwarding:", NC)
    echo_color("  pkill -f 'kubectl port-forward.*neutrino'", NC)
    echo_color("")


@cli.command()
@click.option(
    "--namespace",
    "-n",
    default="default",
    help="Kubernetes namespace (default: default)",
)
@click.option(
    "--all",
    "delete_all",
    is_flag=True,
    default=False,
    help="Delete all resources including ConfigMaps",
)
def down(namespace: str, delete_all: bool) -> None:
    """
    Tear down Neutrino deployment from k3s cluster.

    This command will delete:
    - Deployment (neutrino)
    - Service (neutrino)
    - ConfigMap (neutrino-app-code, if --all is specified)

    Examples:

        neutrino down

        neutrino down --namespace production

        neutrino down --all  # Also delete ConfigMaps
    """

    # Colors for output
    RED = '\033[0;31m'
    GREEN = '\033[0;32m'
    YELLOW = '\033[1;33m'
    NC = '\033[0m'  # No Color

    def echo_color(msg: str, color: str = NC) -> None:
        click.echo(f"{color}{msg}{NC}", err=True)

    def run_command(cmd: str, description: str, check: bool = True) -> subprocess.CompletedProcess:
        """Run a shell command and handle errors."""
        result = subprocess.run(cmd, shell=True, capture_output=True, text=True)
        if check and result.returncode != 0:
            # Don't error if resource doesn't exist
            if "NotFound" not in result.stderr and "not found" not in result.stderr:
                echo_color(f"Warning: {description} - {result.stderr.strip()}", YELLOW)
        else:
            echo_color(f"✓ {description}", GREEN)
        return result

    echo_color("=== Neutrino k3s Teardown ===", GREEN)
    echo_color("")

    # Stop port forwarding first
    echo_color("Stopping port forwarding...", YELLOW)
    result = subprocess.run("pkill -f 'kubectl port-forward.*neutrino'", shell=True, capture_output=True)
    if result.returncode == 0:
        echo_color("✓ Port forwarding stopped", GREEN)
    else:
        echo_color("No active port forwarding found", NC)
    echo_color("")

    # Check kubectl
    if subprocess.run("command -v kubectl", shell=True, capture_output=True).returncode != 0:
        echo_color("Error: kubectl not found. Please install kubectl.", RED)
        sys.exit(1)

    # Check if we can connect to cluster
    result = subprocess.run("kubectl cluster-info", shell=True, capture_output=True)
    if result.returncode != 0:
        echo_color("Error: Cannot connect to Kubernetes cluster. Check your kubeconfig.", RED)
        sys.exit(1)

    echo_color(f"Deleting Neutrino resources from namespace: {namespace}", YELLOW)
    echo_color("")

    # Delete main service
    run_command(
        f"kubectl delete service neutrino --namespace={namespace}",
        "Deleted Service (neutrino)",
        check=False
    )

    # Delete main deployment
    run_command(
        f"kubectl delete deployment neutrino --namespace={namespace}",
        "Deleted Deployment (neutrino)",
        check=False
    )

    # Delete dashboard service
    run_command(
        f"kubectl delete service neutrino-dashboard --namespace={namespace}",
        "Deleted Service (dashboard)",
        check=False
    )

    # Delete dashboard deployment
    run_command(
        f"kubectl delete deployment neutrino-dashboard --namespace={namespace}",
        "Deleted Deployment (dashboard)",
        check=False
    )

    # Delete gateway service
    run_command(
        f"kubectl delete service neutrino-gateway --namespace={namespace}",
        "Deleted Service (gateway)",
        check=False
    )

    # Delete gateway deployment
    run_command(
        f"kubectl delete deployment neutrino-gateway --namespace={namespace}",
        "Deleted Deployment (gateway)",
        check=False
    )

    # Delete PVC for database (shared by dashboard and gateway)
    run_command(
        f"kubectl delete pvc neutrino-db-pvc --namespace={namespace}",
        "Deleted PersistentVolumeClaim (database)",
        check=False
    )

    # Delete ConfigMaps if --all is specified
    if delete_all:
        run_command(
            f"kubectl delete configmap neutrino-app-code --namespace={namespace}",
            "Deleted ConfigMap (neutrino-app-code)",
            check=False
        )
        run_command(
            f"kubectl delete configmap neutrino-config --namespace={namespace}",
            "Deleted ConfigMap (neutrino-config)",
            check=False
        )
    else:
        echo_color("Note: ConfigMaps not deleted. Use --all to delete them.", YELLOW)

    echo_color("")
    echo_color("=== Teardown Complete ===", GREEN)
    echo_color("")
    echo_color("To verify cleanup:", NC)
    echo_color(f"  kubectl get all --namespace={namespace}", NC)
    echo_color(f"  kubectl get pvc --namespace={namespace}", NC)
    echo_color("")


if __name__ == "__main__":
    cli()
