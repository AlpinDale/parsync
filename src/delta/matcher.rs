use std::collections::HashMap;

use anyhow::Result;
use base64::{engine::general_purpose::STANDARD, Engine};

use super::{
    checksum::{strong_hash128, weak_hash, RollingChecksum},
    protocol::{BlockSigWire, DeltaOp, HelperResponse},
};

pub fn build_delta_ops(
    source: &[u8],
    source_mtime_secs: i64,
    block_size: u32,
    blocks: &[BlockSigWire],
) -> Result<HelperResponse> {
    let mut by_weak: HashMap<u32, Vec<&BlockSigWire>> = HashMap::new();
    for block in blocks {
        by_weak.entry(block.weak).or_default().push(block);
    }

    let mut ops = Vec::new();
    let mut i = 0_usize;
    let mut literal_start = 0_usize;
    let bs = block_size as usize;
    let mut copy_bytes = 0_u64;

    let mut rolling = if source.len() >= bs && bs > 0 {
        Some(RollingChecksum::new(&source[..bs]))
    } else {
        None
    };

    while i + bs <= source.len() && bs > 0 {
        let weak = rolling
            .as_ref()
            .map(|r| r.sum())
            .unwrap_or_else(|| weak_hash(&source[i..i + bs]));
        let mut matched: Option<&BlockSigWire> = None;
        if let Some(cands) = by_weak.get(&weak) {
            let strong = strong_hash128(&source[i..i + bs]);
            for cand in cands {
                if cand.len as usize == bs
                    && u128::from_str_radix(&cand.strong_hex, 16).ok() == Some(strong)
                {
                    matched = Some(cand);
                    break;
                }
            }
        }

        if let Some(m) = matched {
            if literal_start < i {
                let lit = &source[literal_start..i];
                ops.push(DeltaOp::Literal {
                    data_b64: STANDARD.encode(lit),
                });
            }
            ops.push(DeltaOp::Copy {
                block_index: m.index,
                len: m.len,
            });
            copy_bytes += m.len as u64;
            i += bs;
            literal_start = i;
            rolling = if i + bs <= source.len() {
                Some(RollingChecksum::new(&source[i..i + bs]))
            } else {
                None
            };
            continue;
        }

        i += 1;
        if let Some(r) = rolling.as_mut() {
            if i + bs <= source.len() {
                r.roll(source[i - 1], source[i + bs - 1]);
            }
        }
    }

    if literal_start < source.len() {
        ops.push(DeltaOp::Literal {
            data_b64: STANDARD.encode(&source[literal_start..]),
        });
    }

    let literal_bytes = (source.len() as u64).saturating_sub(copy_bytes);
    let final_digest = strong_hash128(source);

    Ok(HelperResponse {
        protocol_version: 1,
        source_size: source.len() as u64,
        source_mtime_secs,
        ops,
        final_digest_hex: format!("{final_digest:032x}"),
        literal_bytes,
        copy_bytes,
    })
}

#[cfg(test)]
mod tests {
    use super::build_delta_ops;
    use crate::delta::{build_signature, protocol::BlockSigWire};
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[test]
    fn matcher_emits_copy_and_literal() {
        let mut basis = NamedTempFile::new().expect("tmp");
        basis.write_all(b"abcdefghijklmnop").expect("write");
        let sig = build_signature(basis.path(), 4).expect("sig");
        let blocks: Vec<BlockSigWire> = sig
            .blocks
            .iter()
            .map(|b| BlockSigWire {
                index: b.index,
                len: b.len,
                weak: b.weak,
                strong_hex: format!("{:032x}", b.strong),
            })
            .collect();
        let src = b"abcdZZZZijklmnop";
        let out = build_delta_ops(src, 1, 4, &blocks).expect("build");
        assert!(!out.ops.is_empty());
        assert!(out.copy_bytes > 0);
    }
}
