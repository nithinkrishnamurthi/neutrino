#!/usr/bin/env python3
"""
Automated test runner for multi-resource scheduling.

This script:
1. Generates OpenAPI spec from examples.gpu_resources
2. Starts the Neutrino orchestrator with GPU/CPU worker pools
3. Runs resource scheduling tests
4. Shuts down cleanly
"""

import subprocess
import time
import signal
import sys
import os
from pathlib import Path

def generate_openapi_spec():
    """Generate OpenAPI specification."""
    print("=" * 80)
    print("STEP 1: Generate OpenAPI Specification")
    print("=" * 80)

    cmd = [
        "python3", "-c",
        """
import sys
sys.path.insert(0, 'python')
import json
import examples.gpu_resources
from neutrino import generate_openapi

spec = generate_openapi(title='GPU Resources API', version='1.0.0')

with open('openapi.json', 'w') as f:
    json.dump(spec, f, indent=2)

print('✓ OpenAPI spec saved to openapi.json')
"""
    ]

    result = subprocess.run(cmd, cwd=Path(__file__).parent, capture_output=True, text=True)
    print(result.stdout)
    if result.returncode != 0:
        print(f"✗ Failed to generate OpenAPI spec:")
        print(result.stderr)
        sys.exit(1)

def start_orchestrator():
    """Start the orchestrator in the background."""
    print("\n" + "=" * 80)
    print("STEP 2: Start Orchestrator")
    print("=" * 80)
    print("Starting orchestrator with config: examples/config_gpu.yaml")
    print()

    config_path = Path(__file__).parent / "examples" / "config_gpu.yaml"

    cmd = [
        "cargo", "run", "--release", "--bin", "neutrino-core",
        "--", str(config_path)
    ]

    cwd = Path(__file__).parent

    # Start orchestrator
    proc = subprocess.Popen(
        cmd,
        cwd=cwd,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        text=True,
        bufsize=1
    )

    print(f"Orchestrator PID: {proc.pid}")

    # Wait for orchestrator to be ready
    print("Waiting for orchestrator to start (this may take 10-15 seconds)...")
    ready = False
    start_time = time.time()
    max_wait = 30

    while time.time() - start_time < max_wait:
        # Read output
        if proc.poll() is not None:
            print("\n✗ Orchestrator exited unexpectedly!")
            print("Output:")
            if proc.stdout:
                print(proc.stdout.read())
            sys.exit(1)

        # Check if ready
        try:
            import requests
            resp = requests.get("http://localhost:8080/health", timeout=1)
            if resp.status_code == 200:
                ready = True
                break
        except:
            pass

        time.sleep(0.5)

    if not ready:
        print("\n✗ Orchestrator failed to start within timeout!")
        proc.terminate()
        proc.wait(timeout=5)
        sys.exit(1)

    print("✓ Orchestrator is ready!")
    return proc

def run_tests():
    """Run the test suite."""
    print("\n" + "=" * 80)
    print("STEP 3: Run Resource Scheduling Tests")
    print("=" * 80)
    print()

    cmd = ["python3", "test_resource_scheduling.py"]
    result = subprocess.run(cmd, cwd=Path(__file__).parent)

    return result.returncode == 0

def shutdown_orchestrator(proc):
    """Gracefully shutdown the orchestrator."""
    print("\n" + "=" * 80)
    print("STEP 4: Shutdown Orchestrator")
    print("=" * 80)

    if proc and proc.poll() is None:
        print(f"Shutting down orchestrator (PID {proc.pid})...")
        proc.send_signal(signal.SIGTERM)

        try:
            proc.wait(timeout=10)
            print("✓ Orchestrator shut down gracefully")
        except subprocess.TimeoutExpired:
            print("⚠ Orchestrator did not shut down gracefully, force killing...")
            proc.kill()
            proc.wait()

def main():
    orchestrator_proc = None

    try:
        # Step 1: Generate OpenAPI spec
        generate_openapi_spec()

        # Step 2: Start orchestrator
        orchestrator_proc = start_orchestrator()

        # Step 3: Run tests
        success = run_tests()

        # Step 4: Shutdown
        shutdown_orchestrator(orchestrator_proc)

        if success:
            print("\n" + "=" * 80)
            print("ALL TESTS PASSED ✓")
            print("=" * 80)
            sys.exit(0)
        else:
            print("\n" + "=" * 80)
            print("SOME TESTS FAILED ✗")
            print("=" * 80)
            sys.exit(1)

    except KeyboardInterrupt:
        print("\n\n⚠ Interrupted by user")
        if orchestrator_proc:
            shutdown_orchestrator(orchestrator_proc)
        sys.exit(1)

    except Exception as e:
        print(f"\n\n✗ Error: {e}")
        if orchestrator_proc:
            shutdown_orchestrator(orchestrator_proc)
        sys.exit(1)

if __name__ == "__main__":
    main()
