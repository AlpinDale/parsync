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

The fast path can be tested without RDMA hardware by using the Linux RXE
software RDMA driver (`rdma_rxe`) with rdma-core installed.

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
