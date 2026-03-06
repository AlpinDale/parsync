# Benchmark Suite Design

**Date:** 2026-03-06
**Status:** Approved

## Goal

Add a reproducible benchmark suite that measures parsync's parallel sync performance
across different workload profiles, running in a Docker Alpine container for fast,
portable execution.

## Approach

Custom lightweight benchmark binary (no Criterion) for fast compile and simple output.

## Benchmark Scenarios

| Scenario | Files | Size each | Simulated latency |
|---|---|---|---|
| Many small files | 1000 | 4 KB | 2ms |
| Medium files | 40 | 128 KB | 5ms |
| Few large files | 5 | 10 MB | 10ms |
| Mixed workload | 200 | 1KB-2MB varied | 5ms |

Each scenario tested with **1, 2, 4, 8, 16 jobs**, 5 runs per config.

## Components

### 1. `benches/mock_sync.rs`
- Custom benchmark binary using `DelayedMockRemote` pattern from `tests/perf_smoke.rs`
- Builds mock datasets in-memory, runs `run_sync_with_client()` with varying job counts
- Outputs JSON (structured results) and markdown table to stdout
- Computes median, mean, and speedup vs single-threaded baseline

### 2. `Dockerfile.bench`
- Multi-stage Alpine build:
  - Stage 1: `rust:alpine` — build benchmark binary with musl
  - Stage 2: `alpine:latest` — copy and run binary
- Minimal image size for fast CI

### 3. `scripts/docker_bench.sh`
- Builds Docker image, runs container
- Captures output, saves to `bench-results/`
- Prints markdown table for easy copy-paste

### 4. README update
- New `## Benchmarks` section after "Performance tuning"
- Results table with speedup column
- Reproduction instructions

## Output Format

```
| Scenario                | Jobs | Median (ms) | Mean (ms) | vs 1 job |
|-------------------------|-----:|------------:|----------:|---------:|
| Many small (1000x4KB)   |    1 |        2100 |      2150 |    1.00x |
| Many small (1000x4KB)   |    8 |         310 |       325 |    6.77x |
```

## Non-Goals

- No real SSH/SFTP benchmarks (requires network setup)
- No Criterion (too heavy for fast Docker runs)
- No CI integration (can be added later)
