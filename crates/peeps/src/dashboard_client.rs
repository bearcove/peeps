//! Pull-based dashboard client.
//!
//! When `PEEPS_DASHBOARD=<addr>` is set, connects to the peeps-web server
//! and waits for snapshot requests. On receiving a request, collects a local
//! dump and sends it back as a snapshot reply.

use std::collections::HashMap;

use peeps_types::{SnapshotReply, SnapshotRequest};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tracing::{debug, error, info, warn};

/// Start the background pull loop. Spawns a tracked task that reconnects on failure.
pub fn start_pull_loop(process_name: String, addr: String) {
    peeps_tasks::spawn_tracked("peeps_dashboard_pull", async move {
        loop {
            match TcpStream::connect(&addr).await {
                Ok(stream) => {
                    info!(%addr, "connected to dashboard");
                    if let Err(e) = pull_loop(stream, &process_name).await {
                        warn!(%addr, %e, "dashboard connection lost");
                    }
                }
                Err(e) => {
                    warn!(%addr, %e, "failed to connect to dashboard");
                }
            }
            tokio::time::sleep(std::time::Duration::from_secs(5)).await;
        }
    });
}

/// Read snapshot_request frames, collect dump, send snapshot_reply frames.
async fn pull_loop(stream: TcpStream, process_name: &str) -> std::io::Result<()> {
    let (mut reader, mut writer) = stream.into_split();

    loop {
        // Read length-prefixed frame
        let mut len_buf = [0u8; 4];
        reader.read_exact(&mut len_buf).await?;
        let len = u32::from_be_bytes(len_buf) as usize;

        debug!(frame_len = len, "received frame header from dashboard");

        if len > 128 * 1024 * 1024 {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("frame too large: {len} bytes"),
            ));
        }

        let mut frame = vec![0u8; len];
        reader.read_exact(&mut frame).await?;

        let req: SnapshotRequest = match facet_json::from_slice(&frame) {
            Ok(r) => r,
            Err(e) => {
                warn!(%e, "failed to deserialize snapshot request");
                continue;
            }
        };

        if req.r#type != "snapshot_request" {
            warn!(msg_type = %req.r#type, "ignoring unknown message type");
            continue;
        }

        debug!(snapshot_id = req.snapshot_id, "collecting dump");
        let dump = crate::collect_dump(process_name, HashMap::new());

        let reply = SnapshotReply {
            r#type: "snapshot_reply".to_string(),
            snapshot_id: req.snapshot_id,
            process: process_name.to_string(),
            pid: std::process::id(),
            dump,
        };

        let reply_bytes = facet_json::to_vec(&reply).map_err(|e| {
            std::io::Error::new(std::io::ErrorKind::Other, format!("serialize reply: {e}"))
        })?;

        let frame_len = u32::try_from(reply_bytes.len()).map_err(|_| {
            std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "reply frame exceeds u32 length prefix",
            )
        })?;

        writer.write_all(&frame_len.to_be_bytes()).await?;
        writer.write_all(&reply_bytes).await?;
        writer.flush().await?;

        info!(
            snapshot_id = req.snapshot_id,
            reply_bytes = reply_bytes.len(),
            "sent snapshot reply"
        );
    }
}
