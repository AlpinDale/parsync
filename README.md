# parsync

`parsync` is a high-throughput, resumable sync tool for SSH remotes and
local-to-local transfers, with parallel file transfers, optional block-delta
sync, and a Linux RDMA fast path when both hosts support it.

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

## RDMA fast path

On Linux, SSH transfers can use a direct RDMA fast path for large whole-file
copies when RDMA devices and rdma-core `librdmacm` rsockets are available on
both hosts. `parsync` opens a local RDMA receiver, starts
`parsync --internal-rdma-send` on the source host over SSH, and falls back to
the normal SFTP/chunk transfer if RDMA is not available.
RDMA support, including its CLI controls and internal helper, is not built on
Windows or other non-Linux platforms.

The fast path can be tested without RDMA hardware by using the Linux RXE
software RDMA driver (`rdma_rxe`) with rdma-core installed.
An ignored integration test wraps the VM validation:

```bash
cargo test --test rdma_rxe_vm -- --ignored
```

RDMA is enabled in `auto` mode by default for SSH sources and only applies to
files at least 64 MiB. Useful controls:

```bash
parsync --rdma=require user@example.com:/remote/path /local/destination
parsync --rdma=off user@example.com:/remote/path /local/destination
parsync --rdma-bind 10.10.0.12 user@example.com:/remote/path /local/destination
parsync --rdma-min-size 1048576 user@example.com:/remote/path /local/destination
```

Use `--rdma-bind` when the RDMA fabric uses a different local IPv4 address than
the route selected for SSH. The same settings are available through
`PARSYNC_RDMA`, `PARSYNC_RDMA_BIND`, `PARSYNC_RDMA_MIN_SIZE`, and
`PARSYNC_RDMA_HELPER`, or the config file keys `rdma_mode`, `rdma_bind`,
`rdma_min_size`, and `rdma_helper`.

### Excluding files

Use `--exclude` to skip paths (rsync-style patterns). Patterns are applied on the remote when listing via `find`, or client-side when using the walk fallback.

```bash
parsync -vrPlu --exclude '*.o' --exclude 'build/' --exclude '.git/' user@host:/src /dst
```

- **Basename patterns** (no slash): match any file or directory with that name at any depth, e.g. `*.o`, `node_modules`.
- **Path patterns** (with slash): match that relative path and, for directories, their contents, e.g. `path/to/dir`.
- **Trailing slash**: exclude that directory and everything under it, e.g. `build/`, `.git/`.

Multiple `--exclude` options can be given. Empty patterns are ignored.

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
