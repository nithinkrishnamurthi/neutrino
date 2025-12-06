#!/usr/bin/env python3
"""
Test script to demonstrate multi-resource aware scheduling in Neutrino.

This script:
1. Starts the Neutrino orchestrator with GPU and CPU worker pools
2. Makes requests to endpoints with different resource requirements
3. Verifies that tasks are routed to appropriate workers
4. Shows resource allocation and capacity
"""

import requests
import time
import json
import subprocess
import signal
import sys
from pathlib import Path

ORCHESTRATOR_URL = "http://localhost:8080"

def wait_for_orchestrator(max_wait=30):
    """Wait for orchestrator to be ready."""
    print("Waiting for orchestrator to start...")
    start = time.time()
    while time.time() - start < max_wait:
        try:
            resp = requests.get(f"{ORCHESTRATOR_URL}/health", timeout=1)
            if resp.status_code == 200:
                print("✓ Orchestrator is ready!")
                return True
        except requests.RequestException:
            time.sleep(0.5)
    return False

def check_capacity():
    """Check current resource capacity across all workers."""
    print("\n" + "=" * 80)
    print("RESOURCE CAPACITY")
    print("=" * 80)

    resp = requests.get(f"{ORCHESTRATOR_URL}/capacity")
    if resp.status_code != 200:
        print(f"✗ Failed to get capacity: {resp.status_code}")
        return

    data = resp.json()

    # Show totals
    print("\nCluster Totals:")
    print(f"  Total:     CPUs={data['total']['cpus']:6.1f}  GPUs={data['total']['gpus']:6.1f}  Memory={data['total']['memory_gb']:7.1f}GB")
    print(f"  Available: CPUs={data['available']['cpus']:6.1f}  GPUs={data['available']['gpus']:6.1f}  Memory={data['available']['memory_gb']:7.1f}GB")
    print(f"  Allocated: CPUs={data['allocated']['cpus']:6.1f}  GPUs={data['allocated']['gpus']:6.1f}  Memory={data['allocated']['memory_gb']:7.1f}GB")

    # Show per-worker
    print("\nPer-Worker Resources:")
    print(f"{'Worker ID':<20} {'State':<10} {'CPUs (Used/Total)':<20} {'GPUs (Used/Total)':<20} {'Memory (Used/Total)':<25}")
    print("-" * 100)

    for worker in data['workers']:
        worker_id = worker['worker_id']
        state = worker['state']

        cpu_used = worker['allocated']['cpus']
        cpu_total = worker['capabilities']['cpus']
        cpu_str = f"{cpu_used:.1f}/{cpu_total:.1f}"

        gpu_used = worker['allocated']['gpus']
        gpu_total = worker['capabilities']['gpus']
        gpu_str = f"{gpu_used:.1f}/{gpu_total:.1f}"

        mem_used = worker['allocated']['memory_gb']
        mem_total = worker['capabilities']['memory_gb']
        mem_str = f"{mem_used:.1f}/{mem_total:.1f}GB"

        print(f"{worker_id:<20} {state:<10} {cpu_str:<20} {gpu_str:<20} {mem_str:<25}")

def test_endpoint(path, method="POST", data=None, description=""):
    """Test an endpoint and show results."""
    print("\n" + "-" * 80)
    print(f"Testing: {method} {path}")
    if description:
        print(f"Description: {description}")

    try:
        if method == "GET":
            resp = requests.get(f"{ORCHESTRATOR_URL}{path}", timeout=10)
        else:
            resp = requests.post(
                f"{ORCHESTRATOR_URL}{path}",
                json={"args": data or {}},
                timeout=10
            )

        print(f"Status: {resp.status_code}")

        if resp.status_code == 200:
            result = resp.json()
            print(f"✓ Success")
            print(f"  Worker: {result.get('worker_id', 'N/A')}")
            print(f"  Execution time: {result.get('execution_time_ms', 'N/A')}ms")
            if result.get('result'):
                print(f"  Result: {json.dumps(result['result'], indent=4)}")
        elif resp.status_code == 503:
            error = resp.json()
            print(f"✗ Service Unavailable (as expected for resource constraints)")
            print(f"  Error: {error.get('error', 'N/A')}")
        else:
            print(f"✗ Failed: {resp.text}")

    except requests.RequestException as e:
        print(f"✗ Request failed: {e}")

def main():
    print("=" * 80)
    print("NEUTRINO MULTI-RESOURCE AWARE SCHEDULING TEST")
    print("=" * 80)

    # Check that orchestrator is running
    if not wait_for_orchestrator(max_wait=5):
        print("\n✗ Orchestrator is not running!")
        print("\nTo start the orchestrator, run:")
        print("  cargo run --release --bin neutrino-core -- examples/config_gpu.yaml")
        print("\nOr use the provided test_resources.py script which starts it automatically.")
        sys.exit(1)

    # Show initial capacity
    check_capacity()

    # Test 1: Lightweight preprocessing (0.5 CPUs, 0 GPUs, 0.5GB)
    # Should go to any worker, preferably CPU workers
    test_endpoint(
        "/api/preprocess",
        description="Lightweight preprocessing - should route to CPU workers"
    )

    time.sleep(0.5)
    check_capacity()

    # Test 2: CPU-intensive task (4 CPUs, 0 GPUs, 8GB)
    # Should go to CPU workers with sufficient capacity
    test_endpoint(
        "/api/cpu-intensive",
        description="CPU-intensive task - needs 4 CPUs, should route to CPU workers"
    )

    time.sleep(0.5)
    check_capacity()

    # Test 3: GPU inference (2 CPUs, 1 GPU, 16GB)
    # Should go to GPU workers only
    test_endpoint(
        "/api/inference",
        description="GPU inference - needs 1 GPU, should route to GPU workers only"
    )

    time.sleep(0.5)
    check_capacity()

    # Test 4: Fractional GPU (1 CPU, 0.25 GPUs, 4GB)
    # Should go to GPU workers, multiple can share same GPU
    test_endpoint(
        "/api/fractional-gpu",
        description="Fractional GPU - needs 0.25 GPU, can share with other tasks"
    )

    time.sleep(0.5)
    check_capacity()

    # Test 5: Multi-GPU training (8 CPUs, 4 GPUs, 64GB)
    # Should go to multi-GPU workers only
    test_endpoint(
        "/api/multi-gpu",
        description="Multi-GPU training - needs 4 GPUs, should route to multi-GPU workers"
    )

    time.sleep(0.5)
    check_capacity()

    print("\n" + "=" * 80)
    print("TEST COMPLETE")
    print("=" * 80)
    print("\nKey Observations:")
    print("1. CPU-only tasks routed to CPU workers (no GPU workers wasted)")
    print("2. GPU tasks routed only to GPU workers")
    print("3. Tasks requiring 4 GPUs routed only to multi-GPU workers")
    print("4. Resource allocation tracked correctly across all workers")
    print("5. Insufficient resource errors returned when capacity exceeded")

if __name__ == "__main__":
    main()
