use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HelperRequest {
    pub protocol_version: u32,
    pub source_path: String,
    pub file_size: u64,
    pub mtime_secs: i64,
    pub block_size: u32,
    pub blocks: Vec<BlockSigWire>,
    pub max_literals: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlockSigWire {
    pub index: u64,
    pub len: u32,
    pub weak: u32,
    pub strong_hex: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HelperResponse {
    pub protocol_version: u32,
    pub source_size: u64,
    pub source_mtime_secs: i64,
    pub ops: Vec<DeltaOp>,
    pub final_digest_hex: String,
    pub literal_bytes: u64,
    pub copy_bytes: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind")]
pub enum DeltaOp {
    Copy { block_index: u64, len: u32 },
    Literal { data_b64: String },
}

#[derive(Debug, Clone)]
pub struct DeltaPlan {
    pub ops: Vec<DeltaOp>,
    pub final_digest_hex: String,
    pub literal_bytes: u64,
    pub copy_bytes: u64,
    pub source_size: u64,
    pub source_mtime_secs: i64,
}
