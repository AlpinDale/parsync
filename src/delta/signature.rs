use std::{fs::File, io::Read, path::Path};

use anyhow::{Context, Result};

use super::checksum::{strong_hash128, weak_hash};

#[derive(Debug, Clone)]
pub struct BlockSig {
    pub index: u64,
    pub len: u32,
    pub weak: u32,
    pub strong: u128,
}

#[derive(Debug, Clone)]
pub struct FileSignature {
    pub block_size: u32,
    pub file_size: u64,
    pub blocks: Vec<BlockSig>,
}

pub fn choose_block_size(file_size: u64, configured: Option<u32>) -> u32 {
    if let Some(v) = configured {
        return v.max(1024);
    }
    if file_size < 64 * 1024 * 1024 {
        32 * 1024
    } else if file_size < 1024 * 1024 * 1024 {
        64 * 1024
    } else {
        128 * 1024
    }
}

pub fn build_signature(path: &Path, block_size: u32) -> Result<FileSignature> {
    let mut file =
        File::open(path).with_context(|| format!("open basis file: {}", path.display()))?;
    let mut blocks = Vec::new();
    let mut index = 0_u64;
    let mut total = 0_u64;
    let mut buf = vec![0_u8; block_size as usize];

    loop {
        let n = file
            .read(&mut buf)
            .with_context(|| format!("read basis file: {}", path.display()))?;
        if n == 0 {
            break;
        }
        let chunk = &buf[..n];
        blocks.push(BlockSig {
            index,
            len: n as u32,
            weak: weak_hash(chunk),
            strong: strong_hash128(chunk),
        });
        index += 1;
        total += n as u64;
    }

    Ok(FileSignature {
        block_size,
        file_size: total,
        blocks,
    })
}

#[cfg(test)]
mod tests {
    use std::{fs, io::Write};

    use tempfile::NamedTempFile;

    use super::{build_signature, choose_block_size};

    #[test]
    fn block_size_is_adaptive() {
        assert_eq!(choose_block_size(1024, None), 32 * 1024);
        assert_eq!(choose_block_size(100 * 1024 * 1024, None), 64 * 1024);
        assert_eq!(choose_block_size(2 * 1024 * 1024 * 1024, None), 128 * 1024);
        assert_eq!(choose_block_size(10, Some(2048)), 2048);
    }

    #[test]
    fn signature_builds_blocks() {
        let mut f = NamedTempFile::new().expect("tmp");
        f.write_all(b"abcdefghijklmno").expect("write");
        fs::metadata(f.path()).expect("meta");
        let sig = build_signature(f.path(), 4).expect("sig");
        assert_eq!(sig.blocks.len(), 4);
        assert_eq!(sig.file_size, 15);
    }
}
