#!/usr/bin/env python3
"""
mikrozen-host HTTP Benchmark Script

A comprehensive benchmarking tool similar to wasmCloud's approach using `hey`.
Measures requests/sec, latency percentiles (p50, p90, p99), and compares
cold start vs warm cache performance.

Requirements:
    pip install httpx rich

Usage:
    # Basic benchmark against health endpoint
    uv run tools/benchmark.py

    # Custom endpoint with more options
    uv run tools/benchmark.py --url http://localhost:3000/run/hello --duration 30 --concurrency 100

    # Cold vs warm comparison
    uv run tools/benchmark.py --compare-cold-warm

    # JSON output for CI
    uv run tools/benchmark.py --json > results.json
"""

import argparse
import asyncio
import json
import statistics
import sys
import time
from concurrent.futures import ThreadPoolExecutor
from dataclasses import dataclass, field
from typing import Optional

try:
    import httpx
except ImportError:
    print("Error: httpx not installed. Run: pip install httpx")
    sys.exit(1)

try:
    from rich.console import Console
    from rich.table import Table
    from rich.progress import Progress, SpinnerColumn, TextColumn
    HAS_RICH = True
except ImportError:
    HAS_RICH = False

console = Console() if HAS_RICH else None


@dataclass
class BenchmarkResult:
    """Results from a benchmark run."""
    total_requests: int = 0
    successful_requests: int = 0
    failed_requests: int = 0
    duration_seconds: float = 0.0
    requests_per_second: float = 0.0
    latencies_ms: list = field(default_factory=list)
    errors: list = field(default_factory=list)

    @property
    def p50(self) -> float:
        """50th percentile latency in ms."""
        if not self.latencies_ms:
            return 0.0
        sorted_lat = sorted(self.latencies_ms)
        idx = int(len(sorted_lat) * 0.50)
        return sorted_lat[min(idx, len(sorted_lat) - 1)]

    @property
    def p90(self) -> float:
        """90th percentile latency in ms."""
        if not self.latencies_ms:
            return 0.0
        sorted_lat = sorted(self.latencies_ms)
        idx = int(len(sorted_lat) * 0.90)
        return sorted_lat[min(idx, len(sorted_lat) - 1)]

    @property
    def p99(self) -> float:
        """99th percentile latency in ms."""
        if not self.latencies_ms:
            return 0.0
        sorted_lat = sorted(self.latencies_ms)
        idx = int(len(sorted_lat) * 0.99)
        return sorted_lat[min(idx, len(sorted_lat) - 1)]

    @property
    def avg_latency(self) -> float:
        """Average latency in ms."""
        if not self.latencies_ms:
            return 0.0
        return statistics.mean(self.latencies_ms)

    @property
    def min_latency(self) -> float:
        """Minimum latency in ms."""
        if not self.latencies_ms:
            return 0.0
        return min(self.latencies_ms)

    @property
    def max_latency(self) -> float:
        """Maximum latency in ms."""
        if not self.latencies_ms:
            return 0.0
        return max(self.latencies_ms)

    @property
    def success_rate(self) -> float:
        """Success rate as percentage."""
        if self.total_requests == 0:
            return 0.0
        return (self.successful_requests / self.total_requests) * 100

    def to_dict(self) -> dict:
        """Convert to dictionary for JSON output."""
        return {
            "total_requests": self.total_requests,
            "successful_requests": self.successful_requests,
            "failed_requests": self.failed_requests,
            "duration_seconds": round(self.duration_seconds, 3),
            "requests_per_second": round(self.requests_per_second, 2),
            "success_rate_percent": round(self.success_rate, 2),
            "latency_ms": {
                "min": round(self.min_latency, 3),
                "avg": round(self.avg_latency, 3),
                "max": round(self.max_latency, 3),
                "p50": round(self.p50, 3),
                "p90": round(self.p90, 3),
                "p99": round(self.p99, 3),
            },
            "errors": self.errors[:10] if self.errors else [],
        }


async def run_benchmark(
    url: str,
    duration_seconds: int = 10,
    concurrency: int = 50,
    method: str = "GET",
    body: Optional[str] = None,
    warmup_seconds: int = 2,
    timeout: float = 30.0,
) -> BenchmarkResult:
    """
    Run HTTP benchmark against the specified URL.

    Args:
        url: Target URL to benchmark
        duration_seconds: How long to run the benchmark
        concurrency: Number of concurrent connections
        method: HTTP method (GET, POST, etc.)
        body: Request body for POST/PUT
        warmup_seconds: Warmup duration before measuring
        timeout: Request timeout in seconds

    Returns:
        BenchmarkResult with metrics
    """
    result = BenchmarkResult()
    latencies = []
    errors = []
    running = True

    async def worker(client: httpx.AsyncClient, worker_id: int):
        nonlocal running
        while running:
            start = time.perf_counter()
            try:
                if method.upper() == "POST":
                    response = await client.post(url, content=body, timeout=timeout)
                elif method.upper() == "PUT":
                    response = await client.put(url, content=body, timeout=timeout)
                elif method.upper() == "DELETE":
                    response = await client.delete(url, timeout=timeout)
                else:
                    response = await client.get(url, timeout=timeout)

                elapsed_ms = (time.perf_counter() - start) * 1000

                if response.status_code >= 200 and response.status_code < 400:
                    latencies.append(elapsed_ms)
                else:
                    errors.append(f"HTTP {response.status_code}")

            except httpx.TimeoutException:
                errors.append("timeout")
            except httpx.ConnectError as e:
                errors.append(f"connect_error: {e}")
            except Exception as e:
                errors.append(f"error: {type(e).__name__}")

    # Create client with connection pooling
    limits = httpx.Limits(max_connections=concurrency * 2, max_keepalive_connections=concurrency)

    async with httpx.AsyncClient(limits=limits) as client:
        # Warmup phase
        if warmup_seconds > 0:
            if HAS_RICH:
                console.print(f"[yellow]Warming up for {warmup_seconds}s...[/yellow]")
            else:
                print(f"Warming up for {warmup_seconds}s...")

            warmup_tasks = []
            for i in range(min(concurrency, 10)):  # Use fewer workers for warmup
                warmup_tasks.append(asyncio.create_task(worker(client, i)))

            await asyncio.sleep(warmup_seconds)
            running = False
            await asyncio.gather(*warmup_tasks, return_exceptions=True)

            # Clear warmup metrics
            latencies.clear()
            errors.clear()
            running = True

        # Main benchmark phase
        if HAS_RICH:
            console.print(f"[green]Running benchmark for {duration_seconds}s with {concurrency} connections...[/green]")
        else:
            print(f"Running benchmark for {duration_seconds}s with {concurrency} connections...")

        start_time = time.perf_counter()

        tasks = []
        for i in range(concurrency):
            tasks.append(asyncio.create_task(worker(client, i)))

        await asyncio.sleep(duration_seconds)
        running = False

        # Wait for all tasks to complete
        await asyncio.gather(*tasks, return_exceptions=True)

        end_time = time.perf_counter()

    # Calculate results
    result.duration_seconds = end_time - start_time
    result.latencies_ms = latencies
    result.errors = errors
    result.total_requests = len(latencies) + len(errors)
    result.successful_requests = len(latencies)
    result.failed_requests = len(errors)
    result.requests_per_second = result.successful_requests / result.duration_seconds if result.duration_seconds > 0 else 0

    return result


def print_result(result: BenchmarkResult, title: str = "Benchmark Results"):
    """Print benchmark results in a nice format."""
    if HAS_RICH:
        table = Table(title=title, show_header=True)
        table.add_column("Metric", style="cyan")
        table.add_column("Value", style="green")

        table.add_row("Total Requests", f"{result.total_requests:,}")
        table.add_row("Successful", f"{result.successful_requests:,}")
        table.add_row("Failed", f"{result.failed_requests:,}")
        table.add_row("Duration", f"{result.duration_seconds:.2f}s")
        table.add_row("Requests/sec", f"{result.requests_per_second:,.2f}")
        table.add_row("Success Rate", f"{result.success_rate:.2f}%")
        table.add_row("", "")
        table.add_row("Latency (min)", f"{result.min_latency:.3f}ms")
        table.add_row("Latency (avg)", f"{result.avg_latency:.3f}ms")
        table.add_row("Latency (max)", f"{result.max_latency:.3f}ms")
        table.add_row("Latency (p50)", f"{result.p50:.3f}ms")
        table.add_row("Latency (p90)", f"{result.p90:.3f}ms")
        table.add_row("Latency (p99)", f"{result.p99:.3f}ms")

        console.print(table)

        if result.errors:
            error_counts = {}
            for e in result.errors:
                error_counts[e] = error_counts.get(e, 0) + 1
            console.print("\n[red]Errors:[/red]")
            for error, count in sorted(error_counts.items(), key=lambda x: -x[1])[:5]:
                console.print(f"  {error}: {count}")
    else:
        print(f"\n{'='*60}")
        print(f"  {title}")
        print(f"{'='*60}")
        print(f"  Total Requests:  {result.total_requests:,}")
        print(f"  Successful:      {result.successful_requests:,}")
        print(f"  Failed:          {result.failed_requests:,}")
        print(f"  Duration:        {result.duration_seconds:.2f}s")
        print(f"  Requests/sec:    {result.requests_per_second:,.2f}")
        print(f"  Success Rate:    {result.success_rate:.2f}%")
        print()
        print(f"  Latency (min):   {result.min_latency:.3f}ms")
        print(f"  Latency (avg):   {result.avg_latency:.3f}ms")
        print(f"  Latency (max):   {result.max_latency:.3f}ms")
        print(f"  Latency (p50):   {result.p50:.3f}ms")
        print(f"  Latency (p90):   {result.p90:.3f}ms")
        print(f"  Latency (p99):   {result.p99:.3f}ms")
        print(f"{'='*60}")


async def compare_cold_warm(url: str, concurrency: int = 50, duration: int = 10):
    """
    Compare cold start vs warm cache performance.

    Makes a single request first (cold), waits, then runs benchmark (warm).
    """
    results = {}

    # Cold start: single request after potential server restart
    if HAS_RICH:
        console.print("\n[bold blue]Phase 1: Cold Start Request[/bold blue]")
    else:
        print("\nPhase 1: Cold Start Request")

    async with httpx.AsyncClient() as client:
        start = time.perf_counter()
        try:
            response = await client.get(url, timeout=30.0)
            cold_latency = (time.perf_counter() - start) * 1000
            results["cold_start_ms"] = round(cold_latency, 3)
            results["cold_start_status"] = response.status_code
            if HAS_RICH:
                console.print(f"  Cold start latency: [yellow]{cold_latency:.3f}ms[/yellow]")
            else:
                print(f"  Cold start latency: {cold_latency:.3f}ms")
        except Exception as e:
            results["cold_start_error"] = str(e)
            if HAS_RICH:
                console.print(f"  [red]Cold start error: {e}[/red]")
            else:
                print(f"  Cold start error: {e}")

    # Wait for cache to be warm
    await asyncio.sleep(1)

    # Warm benchmark
    if HAS_RICH:
        console.print("\n[bold blue]Phase 2: Warm Cache Benchmark[/bold blue]")
    else:
        print("\nPhase 2: Warm Cache Benchmark")

    warm_result = await run_benchmark(
        url=url,
        duration_seconds=duration,
        concurrency=concurrency,
        warmup_seconds=2,
    )

    results["warm_cache"] = warm_result.to_dict()
    print_result(warm_result, "Warm Cache Results")

    # Summary comparison
    if "cold_start_ms" in results and warm_result.avg_latency > 0:
        speedup = results["cold_start_ms"] / warm_result.avg_latency
        if HAS_RICH:
            console.print(f"\n[bold]Cache Speedup: [green]{speedup:.1f}x[/green] faster[/bold]")
        else:
            print(f"\nCache Speedup: {speedup:.1f}x faster")

    return results


async def main():
    parser = argparse.ArgumentParser(
        description="mikrozen-host HTTP Benchmark",
        formatter_class=argparse.RawDescriptionHelpFormatter,
        epilog="""
Examples:
  # Basic health check benchmark
  python benchmark.py

  # Custom endpoint
  python benchmark.py --url http://localhost:3000/run/hello

  # High concurrency
  python benchmark.py -c 200 -d 30

  # Compare cold vs warm
  python benchmark.py --compare-cold-warm

  # POST with body
  python benchmark.py -m POST -b '{"key":"value"}' --url http://localhost:3000/run/echo

  # JSON output for CI
  python benchmark.py --json > results.json
        """,
    )

    parser.add_argument(
        "--url", "-u",
        default="http://127.0.0.1:3000/health",
        help="Target URL to benchmark (default: http://127.0.0.1:3000/health)",
    )
    parser.add_argument(
        "--duration", "-d",
        type=int,
        default=10,
        help="Benchmark duration in seconds (default: 10)",
    )
    parser.add_argument(
        "--concurrency", "-c",
        type=int,
        default=50,
        help="Number of concurrent connections (default: 50)",
    )
    parser.add_argument(
        "--method", "-m",
        default="GET",
        choices=["GET", "POST", "PUT", "DELETE"],
        help="HTTP method (default: GET)",
    )
    parser.add_argument(
        "--body", "-b",
        help="Request body for POST/PUT",
    )
    parser.add_argument(
        "--warmup", "-w",
        type=int,
        default=2,
        help="Warmup duration in seconds (default: 2)",
    )
    parser.add_argument(
        "--compare-cold-warm",
        action="store_true",
        help="Compare cold start vs warm cache performance",
    )
    parser.add_argument(
        "--json",
        action="store_true",
        help="Output results as JSON",
    )
    parser.add_argument(
        "--timeout",
        type=float,
        default=30.0,
        help="Request timeout in seconds (default: 30)",
    )

    args = parser.parse_args()

    if not args.json and HAS_RICH:
        console.print("[bold]mikrozen-host HTTP Benchmark[/bold]")
        console.print(f"Target: [cyan]{args.url}[/cyan]")

    if args.compare_cold_warm:
        results = await compare_cold_warm(
            url=args.url,
            concurrency=args.concurrency,
            duration=args.duration,
        )
        if args.json:
            print(json.dumps(results, indent=2))
    else:
        result = await run_benchmark(
            url=args.url,
            duration_seconds=args.duration,
            concurrency=args.concurrency,
            method=args.method,
            body=args.body,
            warmup_seconds=args.warmup,
            timeout=args.timeout,
        )

        if args.json:
            print(json.dumps(result.to_dict(), indent=2))
        else:
            print_result(result)

            # Summary line (easy to parse)
            print(f"\nSummary: {result.requests_per_second:.2f} req/s, "
                  f"{result.avg_latency:.2f}ms avg, {result.p99:.2f}ms p99")


if __name__ == "__main__":
    asyncio.run(main())
