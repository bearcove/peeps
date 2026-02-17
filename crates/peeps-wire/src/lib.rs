use compact_str::CompactString;
use facet::Facet;
use peeps_types::Snapshot;
use std::fmt;

pub const DEFAULT_MAX_FRAME_BYTES: usize = 8 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FrameCodecError {
    PayloadTooLarge { len: usize, max: usize },
    FrameTooShort { len: usize },
    FrameTooLarge { len: usize, max: usize },
    FrameTruncated { expected: usize, actual: usize },
}

impl fmt::Display for FrameCodecError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::PayloadTooLarge { len, max } => {
                write!(f, "payload too large: {len} > {max}")
            }
            Self::FrameTooShort { len } => write!(f, "frame too short: {len}"),
            Self::FrameTooLarge { len, max } => write!(f, "frame too large: {len} > {max}"),
            Self::FrameTruncated { expected, actual } => {
                write!(
                    f,
                    "truncated frame payload: expected {expected}, got {actual}"
                )
            }
        }
    }
}

impl std::error::Error for FrameCodecError {}

#[derive(Debug)]
pub enum WireError {
    Frame(FrameCodecError),
    Json(String),
}

impl fmt::Display for WireError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Frame(err) => write!(f, "{err}"),
            Self::Json(err) => write!(f, "{err}"),
        }
    }
}

impl std::error::Error for WireError {}

impl From<FrameCodecError> for WireError {
    fn from(value: FrameCodecError) -> Self {
        Self::Frame(value)
    }
}

pub fn encode_frame(payload: &[u8], max_payload_bytes: usize) -> Result<Vec<u8>, FrameCodecError> {
    if payload.len() > max_payload_bytes {
        return Err(FrameCodecError::PayloadTooLarge {
            len: payload.len(),
            max: max_payload_bytes,
        });
    }

    let payload_len =
        u32::try_from(payload.len()).map_err(|_| FrameCodecError::PayloadTooLarge {
            len: payload.len(),
            max: u32::MAX as usize,
        })?;

    let mut out = Vec::with_capacity(4 + payload.len());
    out.extend_from_slice(&payload_len.to_be_bytes());
    out.extend_from_slice(payload);
    Ok(out)
}

pub fn encode_frame_default(payload: &[u8]) -> Result<Vec<u8>, FrameCodecError> {
    encode_frame(payload, DEFAULT_MAX_FRAME_BYTES)
}

pub fn decode_frame<'a>(
    frame: &'a [u8],
    max_payload_bytes: usize,
) -> Result<&'a [u8], FrameCodecError> {
    if frame.len() < 4 {
        return Err(FrameCodecError::FrameTooShort { len: frame.len() });
    }

    let mut prefix = [0u8; 4];
    prefix.copy_from_slice(&frame[..4]);
    let payload_len = u32::from_be_bytes(prefix) as usize;
    if payload_len > max_payload_bytes {
        return Err(FrameCodecError::FrameTooLarge {
            len: payload_len,
            max: max_payload_bytes,
        });
    }

    let actual_payload_len = frame.len() - 4;
    if actual_payload_len != payload_len {
        return Err(FrameCodecError::FrameTruncated {
            expected: payload_len,
            actual: actual_payload_len,
        });
    }

    Ok(&frame[4..])
}

pub fn decode_frame_default(frame: &[u8]) -> Result<&[u8], FrameCodecError> {
    decode_frame(frame, DEFAULT_MAX_FRAME_BYTES)
}

#[derive(Facet)]
pub struct Handshake {
    pub process_name: CompactString,
    pub pid: u32,
}

#[derive(Facet)]
pub struct SnapshotRequest {
    pub snapshot_id: i64,
    pub timeout_ms: i64,
}

#[derive(Facet)]
pub struct SnapshotReply {
    pub snapshot_id: i64,
    pub process_name: CompactString,
    pub pid: u32,
    #[facet(skip_unless_truthy)]
    pub snapshot: Option<Snapshot>,
}

#[derive(Facet)]
pub struct ClientError {
    pub process_name: CompactString,
    pub pid: u32,
    pub stage: CompactString,
    pub error: CompactString,
    #[facet(skip_unless_truthy)]
    pub last_frame_utf8: Option<CompactString>,
}

#[derive(Facet)]
#[repr(u8)]
#[facet(rename_all = "snake_case")]
pub enum ClientMessage {
    Handshake(Handshake),
    SnapshotReply(SnapshotReply),
    Error(ClientError),
}

#[derive(Facet)]
#[repr(u8)]
#[facet(rename_all = "snake_case")]
pub enum ServerMessage {
    SnapshotRequest(SnapshotRequest),
}

pub fn encode_client_message(
    message: &ClientMessage,
    max_payload_bytes: usize,
) -> Result<Vec<u8>, WireError> {
    let payload = facet_json::to_vec(message).map_err(|e| WireError::Json(e.to_string()))?;
    Ok(encode_frame(&payload, max_payload_bytes)?)
}

pub fn encode_client_message_default(message: &ClientMessage) -> Result<Vec<u8>, WireError> {
    encode_client_message(message, DEFAULT_MAX_FRAME_BYTES)
}

pub fn decode_client_message(
    frame: &[u8],
    max_payload_bytes: usize,
) -> Result<ClientMessage, WireError> {
    let payload = decode_frame(frame, max_payload_bytes)?;
    facet_json::from_slice(payload).map_err(|e| WireError::Json(e.to_string()))
}

pub fn decode_client_message_default(frame: &[u8]) -> Result<ClientMessage, WireError> {
    decode_client_message(frame, DEFAULT_MAX_FRAME_BYTES)
}

pub fn encode_server_message(
    message: &ServerMessage,
    max_payload_bytes: usize,
) -> Result<Vec<u8>, WireError> {
    let payload = facet_json::to_vec(message).map_err(|e| WireError::Json(e.to_string()))?;
    Ok(encode_frame(&payload, max_payload_bytes)?)
}

pub fn encode_server_message_default(message: &ServerMessage) -> Result<Vec<u8>, WireError> {
    encode_server_message(message, DEFAULT_MAX_FRAME_BYTES)
}

pub fn decode_server_message(
    frame: &[u8],
    max_payload_bytes: usize,
) -> Result<ServerMessage, WireError> {
    let payload = decode_frame(frame, max_payload_bytes)?;
    facet_json::from_slice(payload).map_err(|e| WireError::Json(e.to_string()))
}

pub fn decode_server_message_default(frame: &[u8]) -> Result<ServerMessage, WireError> {
    decode_server_message(frame, DEFAULT_MAX_FRAME_BYTES)
}
