use std::{
    fs,
    io::{self, Read, Write},
    path::PathBuf,
    time::UNIX_EPOCH,
};

use anyhow::{Context, Result};

use crate::delta::{build_delta_ops, protocol::HelperRequest};

pub fn run_stdio() -> Result<()> {
    let mut input = String::new();
    io::stdin()
        .read_to_string(&mut input)
        .context("read stdin")?;
    let req: HelperRequest = serde_json::from_str(&input).context("parse request json")?;
    if req.protocol_version != 1 {
        anyhow::bail!("unsupported protocol version: {}", req.protocol_version);
    }

    let path = PathBuf::from(req.source_path.clone());
    let bytes = fs::read(&path).with_context(|| format!("read source file: {}", path.display()))?;
    let meta =
        fs::metadata(&path).with_context(|| format!("stat source file: {}", path.display()))?;
    let mtime_secs = meta
        .modified()
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);

    let resp = build_delta_ops(&bytes, mtime_secs, req.block_size, &req.blocks)?;
    let out = serde_json::to_vec(&resp).context("serialize response")?;
    io::stdout().write_all(&out).context("write stdout")?;
    Ok(())
}
