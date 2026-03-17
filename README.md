# parsync

`parsync` is a high-throughput, resumable sync tool for SSH remotes and
local-to-local transfers, with parallel file transfers and optional block-delta
sync.

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
