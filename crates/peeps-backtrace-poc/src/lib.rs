use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct TraceBundle {
    pub schema_version: u32,
    pub capture_binary: String,
    pub traces: Vec<TraceRecord>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct TraceRecord {
    pub label: String,
    pub frames: Vec<FrameRecord>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct FrameRecord {
    pub ip: u64,
    pub module_base: Option<u64>,
    pub module_path: Option<String>,
}
