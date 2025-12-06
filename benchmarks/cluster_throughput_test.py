#!/usr/bin/env python3
"""
Neutrino Cluster Performance Test

This script measures throughput, latency, and resource routing performance
of a deployed Neutrino cluster.

Usage:
    python benchmarks/cluster_throughput_test.py --host http://192.168.0.35:8080

Requirements:
    pip install aiohttp click rich
"""

import asyncio
import time
import statistics
from typing import List, Dict, Any
from dataclasses import dataclass, field
import json

import aiohttp
import click
from rich.console import Console
from rich.table import Table
from rich.progress import Progress, SpinnerColumn, TextColumn, BarColumn, TaskProgressColumn

console = Console()


@dataclass
class TestResult:
    """Results from a single endpoint test"""
    endpoint: str
    method: str
    total_requests: int
    successful: int
    failed: int
    duration_secs: float
    latencies_ms: List[float] = field(default_factory=list)

    @property
    def requests_per_sec(self) -> float:
        return self.successful / self.duration_secs if self.duration_secs > 0 else 0

    @property
    def p50_latency(self) -> float:
        return statistics.median(self.latencies_ms) if self.latencies_ms else 0

    @property
    def p95_latency(self) -> float:
        if not self.latencies_ms:
            return 0
        sorted_latencies = sorted(self.latencies_ms)
        idx = int(len(sorted_latencies) * 0.95)
        return sorted_latencies[idx]

    @property
    def p99_latency(self) -> float:
        if not self.latencies_ms:
            return 0
        sorted_latencies = sorted(self.latencies_ms)
        idx = int(len(sorted_latencies) * 0.99)
        return sorted_latencies[idx]

    @property
    def avg_latency(self) -> float:
        return statistics.mean(self.latencies_ms) if self.latencies_ms else 0

    @property
    def success_rate(self) -> float:
        return (self.successful / self.total_requests * 100) if self.total_requests > 0 else 0


async def make_request(
    session: aiohttp.ClientSession,
    method: str,
    url: str,
    json_data: Dict[str, Any] = None
) -> tuple[bool, float]:
    """Make a single HTTP request and return (success, latency_ms)"""
    start = time.perf_counter()
    try:
        async with session.request(method, url, json=json_data, timeout=aiohttp.ClientTimeout(total=30)) as resp:
            await resp.read()  # Consume response body
            latency_ms = (time.perf_counter() - start) * 1000
            return (resp.status in [200, 201], latency_ms)
    except Exception as e:
        latency_ms = (time.perf_counter() - start) * 1000
        return (False, latency_ms)


async def test_endpoint(
    host: str,
    endpoint: str,
    method: str,
    num_requests: int,
    concurrency: int,
    json_data: Dict[str, Any] = None,
    progress: Progress = None,
    task_id: int = None
) -> TestResult:
    """Test a single endpoint with concurrent requests"""
    url = f"{host}{endpoint}"
    result = TestResult(
        endpoint=endpoint,
        method=method,
        total_requests=num_requests,
        successful=0,
        failed=0,
        duration_secs=0,
    )

    connector = aiohttp.TCPConnector(limit=concurrency)
    async with aiohttp.ClientSession(connector=connector) as session:
        start_time = time.perf_counter()

        # Create batches of concurrent requests
        batch_size = concurrency
        for i in range(0, num_requests, batch_size):
            batch = min(batch_size, num_requests - i)
            tasks = [make_request(session, method, url, json_data) for _ in range(batch)]
            results = await asyncio.gather(*tasks)

            for success, latency in results:
                if success:
                    result.successful += 1
                else:
                    result.failed += 1
                result.latencies_ms.append(latency)

            if progress and task_id is not None:
                progress.update(task_id, completed=i + batch)

        result.duration_secs = time.perf_counter() - start_time

    return result


async def run_all_tests(host: str, num_requests: int, concurrency: int) -> List[TestResult]:
    """Run all performance tests"""

    # Define test cases
    test_cases = [
        # FastAPI fallback routes (simple CRUD, no resource requirements)
        {
            "endpoint": "/api/users",
            "method": "GET",
            "json_data": None,
            "description": "GET /api/users (FastAPI - no resource routing)",
        },
        {
            "endpoint": "/users/1",
            "method": "GET",
            "json_data": None,
            "description": "GET /users/1 (FastAPI - no resource routing)",
        },
        {
            "endpoint": "/health",
            "method": "GET",
            "json_data": None,
            "description": "GET /health (Health check)",
        },
        # Neutrino routes with resource requirements (routed based on OpenAPI spec)
        {
            "endpoint": "/neutrino/process",
            "method": "POST",
            "json_data": {"text": "hello world", "iterations": 100},
            "description": "POST /neutrino/process (1 CPU, 0 GPU, 1GB RAM)",
        },
        {
            "endpoint": "/neutrino/analyze",
            "method": "POST",
            "json_data": {"user_id": 1, "data": [1.5, 2.3, 3.7, 4.2, 5.8, 6.1, 7.3, 8.9]},
            "description": "POST /neutrino/analyze (1 CPU, 0 GPU, 1GB RAM)",
        },
    ]

    results = []

    with Progress(
        SpinnerColumn(),
        TextColumn("[progress.description]{task.description}"),
        BarColumn(),
        TaskProgressColumn(),
        console=console,
    ) as progress:

        for test_case in test_cases:
            task = progress.add_task(
                f"Testing {test_case['description']}...",
                total=num_requests
            )

            result = await test_endpoint(
                host=host,
                endpoint=test_case["endpoint"],
                method=test_case["method"],
                num_requests=num_requests,
                concurrency=concurrency,
                json_data=test_case.get("json_data"),
                progress=progress,
                task_id=task,
            )

            results.append(result)
            progress.update(task, completed=num_requests)

    return results


def print_results(results: List[TestResult]):
    """Print formatted test results"""
    console.print("\n")
    console.print("[bold cyan]╔═══════════════════════════════════════════════════════════════╗[/bold cyan]")
    console.print("[bold cyan]║           NEUTRINO CLUSTER PERFORMANCE RESULTS               ║[/bold cyan]")
    console.print("[bold cyan]╚═══════════════════════════════════════════════════════════════╝[/bold cyan]")
    console.print()

    # Summary table
    table = Table(title="Throughput & Latency Metrics", show_header=True, header_style="bold magenta")
    table.add_column("Endpoint", style="cyan", width=25)
    table.add_column("Method", justify="center", width=8)
    table.add_column("Requests", justify="right", width=10)
    table.add_column("RPS", justify="right", width=10)
    table.add_column("Success %", justify="right", width=10)
    table.add_column("P50 (ms)", justify="right", width=10)
    table.add_column("P95 (ms)", justify="right", width=10)
    table.add_column("P99 (ms)", justify="right", width=10)

    total_requests = 0
    total_successful = 0
    total_duration = 0

    for result in results:
        table.add_row(
            result.endpoint,
            result.method,
            str(result.successful),
            f"{result.requests_per_sec:.1f}",
            f"{result.success_rate:.1f}%",
            f"{result.p50_latency:.1f}",
            f"{result.p95_latency:.1f}",
            f"{result.p99_latency:.1f}",
        )
        total_requests += result.total_requests
        total_successful += result.successful
        total_duration += result.duration_secs

    console.print(table)
    console.print()

    # Overall statistics
    avg_rps = total_successful / (total_duration / len(results)) if total_duration > 0 else 0
    overall_success_rate = (total_successful / total_requests * 100) if total_requests > 0 else 0

    stats_table = Table(title="Overall Statistics", show_header=True, header_style="bold green")
    stats_table.add_column("Metric", style="yellow")
    stats_table.add_column("Value", justify="right", style="green")

    stats_table.add_row("Total Requests", str(total_requests))
    stats_table.add_row("Successful Requests", str(total_successful))
    stats_table.add_row("Failed Requests", str(total_requests - total_successful))
    stats_table.add_row("Overall Success Rate", f"{overall_success_rate:.2f}%")
    stats_table.add_row("Average RPS (per endpoint)", f"{avg_rps:.1f}")
    stats_table.add_row("Total Test Duration", f"{sum(r.duration_secs for r in results):.2f}s")

    console.print(stats_table)
    console.print()

    # Detailed latency breakdown
    console.print("[bold yellow]Latency Breakdown (all endpoints):[/bold yellow]")
    all_latencies = []
    for result in results:
        all_latencies.extend(result.latencies_ms)

    if all_latencies:
        latency_table = Table(show_header=False)
        latency_table.add_column("Percentile", style="cyan")
        latency_table.add_column("Latency", justify="right", style="green")

        sorted_latencies = sorted(all_latencies)
        latency_table.add_row("Min", f"{min(all_latencies):.2f} ms")
        latency_table.add_row("P50 (Median)", f"{statistics.median(sorted_latencies):.2f} ms")
        latency_table.add_row("P95", f"{sorted_latencies[int(len(sorted_latencies) * 0.95)]:.2f} ms")
        latency_table.add_row("P99", f"{sorted_latencies[int(len(sorted_latencies) * 0.99)]:.2f} ms")
        latency_table.add_row("Max", f"{max(all_latencies):.2f} ms")
        latency_table.add_row("Mean", f"{statistics.mean(all_latencies):.2f} ms")
        latency_table.add_row("StdDev", f"{statistics.stdev(all_latencies):.2f} ms")

        console.print(latency_table)
    console.print()


async def check_cluster_health(host: str) -> bool:
    """Check if the cluster is healthy before running tests"""
    console.print(f"[yellow]Checking cluster health at {host}...[/yellow]")

    try:
        async with aiohttp.ClientSession() as session:
            # Try to hit the health endpoint
            async with session.get(f"{host}/health", timeout=aiohttp.ClientTimeout(total=5)) as resp:
                if resp.status == 200:
                    data = await resp.json()
                    console.print(f"[green]✓ Cluster is healthy: {data}[/green]")
                    return True
                else:
                    console.print(f"[red]✗ Cluster returned status {resp.status}[/red]")
                    return False
    except Exception as e:
        console.print(f"[red]✗ Cannot connect to cluster: {e}[/red]")
        console.print("[yellow]Attempting to test anyway...[/yellow]")
        return True  # Continue anyway


@click.command()
@click.option(
    "--host",
    default="http://192.168.0.35:8080",
    help="Neutrino cluster host (default: http://192.168.0.35:8080)",
)
@click.option(
    "--requests",
    default=1000,
    help="Number of requests per endpoint (default: 1000)",
)
@click.option(
    "--concurrency",
    default=50,
    help="Number of concurrent requests (default: 50)",
)
@click.option(
    "--output",
    type=click.Path(),
    help="Output file for JSON results (optional)",
)
def main(host: str, requests: int, concurrency: int, output: str):
    """Run performance tests against Neutrino cluster"""

    console.print("[bold blue]Starting Neutrino Cluster Performance Test[/bold blue]")
    console.print(f"[cyan]Host:[/cyan] {host}")
    console.print(f"[cyan]Requests per endpoint:[/cyan] {requests}")
    console.print(f"[cyan]Concurrency:[/cyan] {concurrency}")
    console.print()

    # Check cluster health
    asyncio.run(check_cluster_health(host))

    # Run tests
    console.print("[yellow]Running performance tests...[/yellow]")
    results = asyncio.run(run_all_tests(host, requests, concurrency))

    # Print results
    print_results(results)

    # Save to JSON if requested
    if output:
        output_data = {
            "config": {
                "host": host,
                "requests_per_endpoint": requests,
                "concurrency": concurrency,
                "timestamp": time.strftime("%Y-%m-%d %H:%M:%S"),
            },
            "results": [
                {
                    "endpoint": r.endpoint,
                    "method": r.method,
                    "total_requests": r.total_requests,
                    "successful": r.successful,
                    "failed": r.failed,
                    "duration_secs": r.duration_secs,
                    "requests_per_sec": r.requests_per_sec,
                    "success_rate": r.success_rate,
                    "latency_ms": {
                        "min": min(r.latencies_ms) if r.latencies_ms else 0,
                        "p50": r.p50_latency,
                        "p95": r.p95_latency,
                        "p99": r.p99_latency,
                        "max": max(r.latencies_ms) if r.latencies_ms else 0,
                        "mean": r.avg_latency,
                        "stddev": statistics.stdev(r.latencies_ms) if len(r.latencies_ms) > 1 else 0,
                    },
                }
                for r in results
            ],
        }

        with open(output, "w") as f:
            json.dump(output_data, f, indent=2)

        console.print(f"[green]Results saved to {output}[/green]")


if __name__ == "__main__":
    main()
