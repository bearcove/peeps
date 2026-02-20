use facet::Facet;
use moire_types::{CutAck, CutRequest, PullChangesResponse, Snapshot};
use std::fmt;

pub const DEFAULT_MAX_FRAME_BYTES: usize = 128 * 1024 * 1024;

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
pub struct TraceCapabilities {
    pub trace_v1: bool,
    pub requires_frame_pointers: bool,
    pub sampling_supported: bool,
    pub alloc_tracking_supported: bool,
}

#[derive(Facet)]
pub struct ModuleManifestEntry {
    pub module_path: String,
    pub runtime_base: u64,
    pub build_id: String,
    pub debug_id: String,
    pub arch: String,
}

#[derive(Facet)]
pub struct Handshake {
    pub process_name: String,
    pub pid: u32,
    pub trace_capabilities: TraceCapabilities,
    pub module_manifest: Vec<ModuleManifestEntry>,
}

#[derive(Facet)]
pub struct SnapshotRequest {
    pub snapshot_id: i64,
    pub timeout_ms: i64,
}

#[derive(Facet)]
pub struct SnapshotReply {
    pub snapshot_id: i64,
    /// Process-relative milliseconds at the moment the process assembled this snapshot.
    pub ptime_now_ms: u64,
    #[facet(skip_unless_truthy)]
    pub snapshot: Option<Snapshot>,
}

#[derive(Facet)]
pub struct ClientError {
    pub process_name: String,
    pub pid: u32,
    pub stage: String,
    pub error: String,
    #[facet(skip_unless_truthy)]
    pub last_frame_utf8: Option<String>,
}

#[derive(Facet)]
#[repr(u8)]
#[facet(rename_all = "snake_case")]
pub enum ClientMessage {
    Handshake(Handshake),
    SnapshotReply(SnapshotReply),
    DeltaBatch(PullChangesResponse),
    CutAck(CutAck),
    Error(ClientError),
}

#[derive(Facet)]
#[repr(u8)]
#[facet(rename_all = "snake_case")]
pub enum ServerMessage {
    SnapshotRequest(SnapshotRequest),
    CutRequest(CutRequest),
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

#[cfg(test)]
mod tests {
    use super::*;
    use moire_types::{CutId, SeqNo, Snapshot, StreamCursor, StreamId};

    fn client_payload_json(message: &ClientMessage) -> String {
        let frame = encode_client_message_default(message).expect("client frame should encode");
        let payload = decode_frame_default(&frame).expect("frame should decode");
        std::str::from_utf8(payload)
            .expect("payload should be utf8 json")
            .to_string()
    }

    fn server_payload_json(message: &ServerMessage) -> String {
        let frame = encode_server_message_default(message).expect("server frame should encode");
        let payload = decode_frame_default(&frame).expect("frame should decode");
        std::str::from_utf8(payload)
            .expect("payload should be utf8 json")
            .to_string()
    }

    #[test]
    fn client_handshake_wire_shape() {
        let json = client_payload_json(&ClientMessage::Handshake(Handshake {
            process_name: "vixenfs-swift".into(),
            pid: 42,
            trace_capabilities: TraceCapabilities {
                trace_v1: true,
                requires_frame_pointers: true,
                sampling_supported: false,
                alloc_tracking_supported: false,
            },
            module_manifest: vec![ModuleManifestEntry {
                module_path: "/usr/lib/libvixenfs_swift.dylib".into(),
                runtime_base: 4_294_967_296,
                build_id: "buildid:abc123".into(),
                debug_id: "debugid:def456".into(),
                arch: "aarch64".into(),
            }],
        }));
        assert_eq!(
            json,
            r#"{"handshake":{"process_name":"vixenfs-swift","pid":42,"trace_capabilities":{"trace_v1":true,"requires_frame_pointers":true,"sampling_supported":false,"alloc_tracking_supported":false},"module_manifest":[{"module_path":"/usr/lib/libvixenfs_swift.dylib","runtime_base":4294967296,"build_id":"buildid:abc123","debug_id":"debugid:def456","arch":"aarch64"}]}}"#
        );
    }

    #[test]
    fn client_snapshot_reply_wire_shape() {
        let json = client_payload_json(&ClientMessage::SnapshotReply(SnapshotReply {
            snapshot_id: 7,
            ptime_now_ms: 1234,
            snapshot: Some(Snapshot {
                sources: vec![],
                entities: vec![],
                scopes: vec![],
                edges: vec![],
                events: vec![],
            }),
        }));
        assert_eq!(
            json,
            r#"{"snapshot_reply":{"snapshot_id":7,"ptime_now_ms":1234,"snapshot":{"sources":[],"entities":[],"scopes":[],"edges":[],"events":[]}}}"#
        );
    }

    #[test]
    fn client_cut_ack_wire_shape() {
        let json = client_payload_json(&ClientMessage::CutAck(moire_types::CutAck {
            cut_id: CutId("cut-1".into()),
            cursor: StreamCursor {
                stream_id: StreamId("vixenfs-swift-42".into()),
                next_seq_no: SeqNo(0),
            },
        }));
        assert_eq!(
            json,
            r#"{"cut_ack":{"cut_id":"cut-1","cursor":{"stream_id":"vixenfs-swift-42","next_seq_no":0}}}"#
        );
    }

    #[test]
    fn server_snapshot_request_wire_shape() {
        let json = server_payload_json(&ServerMessage::SnapshotRequest(SnapshotRequest {
            snapshot_id: 7,
            timeout_ms: 5000,
        }));
        assert_eq!(
            json,
            r#"{"snapshot_request":{"snapshot_id":7,"timeout_ms":5000}}"#
        );
    }

    #[test]
    fn server_cut_request_wire_shape() {
        let json = server_payload_json(&ServerMessage::CutRequest(moire_types::CutRequest {
            cut_id: CutId("cut-1".into()),
        }));
        assert_eq!(json, r#"{"cut_request":{"cut_id":"cut-1"}}"#);
    }
}
