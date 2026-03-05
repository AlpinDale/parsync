# prsync

`prsync` is a high-throughput, resumable pull syncs from SSH remotes. In
essence, a parallelized `rsync` implementation.

## Building

Make sure you have Rust stable installed (via rustup), then:

```bash
make build
make install
```

At the moment, only Linux and macOS are supported.

## Usage

```bash
prsync -vrPlu user@example.com:/remote/path /local/destination
```

To specify a non-default SSH port:

```bash
prsync -vrPlu user@example.com:2222:/remote/path /local/destination
```

Reading the hostname from SSH config is also supported.

## Key behavior

- `-r`: recursive directory sync
- `-v`: verbose summary
- `-P`: partial + progress mode
- `-l`: preserve symlinks
- `-u`: skip if destination file is newer than source
- `--resume` / `--no-resume`: explicit resume policy override
- Resume state is stored in `/local/destination/.prsync/state.db`
- Active run lock is `/local/destination/.prsync/lock`

## Performance knobs

```bash
prsync -vrPlu --jobs 16 --chunk-size 16777216 --chunk-threshold 134217728 user@host:/src /dst
```

## Delta mode (opt-in)

Enable rsync-class block deltas for large changed files:

```bash
prsync -vrPlu --delta --delta-min-size 8388608 user@host:/src /dst
```

Delta flags:

- `--delta`: enable delta transfer
- `--delta-min-size`: minimum file size eligible for deltas
- `--delta-block-size`: fixed block size (auto if omitted)
- `--delta-max-literals`: fallback to full transfer when unmatched literals exceed threshold
- `--delta-helper`: remote helper command (default `prsync --internal-remote-helper`)
- `--no-delta-fallback`: fail if delta path fails (instead of full-transfer fallback)

Remote helper deployment:

1. Build locally: `cargo build --release --bin prsync`
2. Copy the same `prsync` binary to remote PATH.
3. Delta mode invokes remote helper via `prsync --internal-remote-helper --stdio`.

## Config and precedence

- Optional config file: `~/.config/prsync/config.toml`
- Supported keys: `jobs`, `chunk_size`, `chunk_threshold`, `retries`, `resume`, `state_dir`, `delta_enabled`, `delta_min_size`, `delta_block_size`, `delta_max_literals`, `delta_helper`, `delta_fallback`
- Environment overrides: `PRSYNC_JOBS`, `PRSYNC_CHUNK_SIZE`, `PRSYNC_CHUNK_THRESHOLD`, `PRSYNC_RETRIES`, `PRSYNC_RESUME`, `PRSYNC_STATE_DIR`, `PRSYNC_DELTA`, `PRSYNC_DELTA_MIN_SIZE`, `PRSYNC_DELTA_BLOCK_SIZE`, `PRSYNC_DELTA_MAX_LITERALS`, `PRSYNC_DELTA_HELPER`, `PRSYNC_DELTA_FALLBACK`
The order of precedence is CLI > env > config file > built-in defaults.

You can override state location explicitly:

```bash
prsync -vrPlu --state-dir /var/tmp/prsync-state user@host:/src /dst
```

## Metadata flags

Optional metadata preservation beyond `-vrPlu`:

- `-p`: permissions
- `-o`: owner
- `-g`: group
- `-A`: ACLs (`getfacl` on remote, `setfacl` on local)
- `-X`: xattrs (`getfattr` on remote, local xattr apply)
-
