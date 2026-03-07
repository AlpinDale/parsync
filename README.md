# parsync

`parsync` is a high-throughput, resumable pull sync from SSH remotes, with
parallel file transfers and optional block-delta sync.

![demo](assets/demo.gif)

## Installation

**Linux and macOS:**

```bash
curl -fsSL https://alpindale.net/install.sh | bash
```

**Windows:**

```powershell
powershell -ExecutionPolicy Bypass -c "irm https://alpindale.net/install.ps1 | iex"
```

You can also install with cargo:

```bash
cargo install parsync
```

You may also download the binary for your platform from the
[releases page](https://github.com/AlpinDale/parsync/releases), or install from source:

```bash
make build
make install
```

## Platform support

- Linux: `x86_64-unknown-linux-gnu`, `aarch64-unknown-linux-gnu`
- macOS: `aarch64-apple-darwin`, `x86_64-apple-darwin`
- Windows: `x86_64-pc-windows-msvc` (best-effort metadata support)

## Usage

```bash
parsync -vrPlu user@example.com:/remote/path /local/destination
```

With non-default SSH port:

```bash
parsync -vrPlu user@example.com:2222:/remote/path /local/destination
```

SSH config host aliases are supported.

## Performance tuning

```bash
parsync -vrPlu --jobs 16 --chunk-size 16777216 --chunk-threshold 134217728 user@host:/src /dst
```

Balanced mode defaults:

- no per-file `sync_all` barriers (atomic rename preserved)
- existing-file digest checks are skipped unless requested
- chunk completion state is committed in batches
- post-transfer remote mutation `stat` check is skipped (enabled in strict mode)

Throughput flags:

- `--strict-durability`: enable fsync-heavy strict mode
- `--verify-existing`: hash existing files before skip decisions
- `--sftp-read-concurrency`: parallel per-file read requests for large files
- `--sftp-read-chunk-size`: read request size for SFTP range pulls

## Benchmarks

Measured using an in-process mock remote with simulated per-read latency
(no real SSH overhead). Each configuration runs 5 times; the table shows the
median wall-clock time and speedup relative to a single worker.

**Environment:** Docker `alpine:latest`, compiled with `rust:alpine` (musl).

### Scaling by workload

| Scenario | Jobs | Median (ms) | Mean (ms) | vs 1 job |
|---|---:|---:|---:|---:|
| **Many small (1000 x 4 KB, 2 ms latency)** | | | | |
| | 1 | 3203.2 | 3186.6 | 1.00x |
| | 2 | 1762.5 | 1764.2 | 1.82x |
| | 4 | 1120.3 | 1132.3 | 2.86x |
| | 8 | 806.3 | 812.0 | 3.97x |
| | 16 | 718.2 | 748.1 | 4.46x |
| **Medium (40 x 128 KB, 5 ms latency)** | | | | |
| | 1 | 487.9 | 488.5 | 1.00x |
| | 2 | 286.6 | 285.8 | 1.70x |
| | 4 | 170.7 | 180.7 | 2.86x |
| | 8 | 98.7 | 111.0 | 4.94x |
| | 16 | 106.6 | 102.5 | 4.58x |
| **Few large (5 x 10 MB, 10 ms latency)** | | | | |
| | 1 | 8301.7 | 8301.7 | 1.00x |
| | 2 | 5024.6 | 5160.1 | 1.65x |
| | 4 | 3346.7 | 3355.3 | 2.48x |
| | 8 | 1712.6 | 1795.3 | 4.85x |
| | 16 | 1722.1 | 1725.8 | 4.82x |
| **Mixed (200 files, 1 KB-2 MB, 5 ms latency)** | | | | |
| | 1 | 8508.5 | 8513.8 | 1.00x |
| | 2 | 4477.8 | 4657.2 | 1.90x |
| | 4 | 2223.7 | 2229.5 | 3.83x |
| | 8 | 1205.7 | 1206.6 | 7.06x |
| | 16 | 727.4 | 739.7 | **11.70x** |

### Key takeaways

- Mixed workloads benefit the most from parallelism (up to **11.7x** with 16 jobs)
- Large-file scenarios plateau around 8 jobs (I/O bound on chunk writes)
- Small-file scenarios show diminishing returns past 8 jobs (scheduling overhead)
- Medium workloads hit peak throughput at 8 jobs

### Reproduce

```bash
# Run benchmarks in Docker Alpine
./scripts/docker_bench.sh

# Or build and run the benchmark binary directly
cargo bench --bench mock_sync
```

### Notes on Windows metadata behavior

- `-A`, `-X`: warn and continue (unsupported)
- `-o`, `-g`: warn and continue (unsupported)
- `-p`: best-effort (readonly mapping), then continue
- `-l`: attempts symlink creation; if OS/privilege disallows it, symlink is skipped with warning

Enable strict mode to hard-fail on unsupported behavior:

```bash
parsync --strict-windows-metadata -vrPlu user@host:/src C:\\dst
```

## Windows symlink troubleshooting

Windows symlink creation usually requires one of:

- Administrator privileges
- Developer Mode enabled

If not available, `-l` may skip symlinks (or fail with `--strict-windows-metadata`).
