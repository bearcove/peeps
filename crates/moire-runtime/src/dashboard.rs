use moire_types::SeqNo;
use std::sync::OnceLock;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::time::MissedTickBehavior;

use moire_wire::{
    ClientMessage, ServerMessage, decode_server_message_default, encode_client_message_default,
};

use super::api::{ack_cut, pull_changes_since};
use super::{DASHBOARD_PUSH_INTERVAL_MS, DASHBOARD_PUSH_MAX_CHANGES, DASHBOARD_RECONNECT_DELAY_MS};

pub(super) fn init_dashboard_push_loop(process_name: &str) {
    static STARTED: OnceLock<()> = OnceLock::new();
    if STARTED.set(()).is_err() {
        return;
    }

    // r[impl config.dashboard-addr]
    let Some(addr) = std::env::var("MOIRE_DASHBOARD")
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
    else {
        return;
    };

    let process_name = String::from(process_name);

    if tokio::runtime::Handle::try_current().is_ok() {
        tokio::spawn(async move {
            run_dashboard_push_loop(addr, process_name).await;
        });
        return;
    }

    std::thread::spawn(move || {
        if let Ok(rt) = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
        {
            rt.block_on(async move {
                run_dashboard_push_loop(addr, process_name).await;
            });
        }
    });
}

async fn run_dashboard_push_loop(addr: String, process_name: String) {
    loop {
        let connected = run_dashboard_session(&addr, process_name.clone()).await;
        let _ = connected;
        // r[impl config.dashboard-reconnect]
        tokio::time::sleep(Duration::from_millis(DASHBOARD_RECONNECT_DELAY_MS)).await;
    }
}

async fn run_dashboard_session(addr: &str, process_name: String) -> Result<(), String> {
    let stream = TcpStream::connect(addr)
        .await
        .map_err(|e| format!("dashboard connect: {e}"))?;
    let (mut reader, mut writer) = stream.into_split();

    // r[impl wire.magic]
    writer
        .write_all(&moire_wire::encode_protocol_magic())
        .await
        .map_err(|e| format!("write protocol magic: {e}"))?;

    let mut last_sent_manifest_revision = u64::MAX;
    send_handshake_if_manifest_changed(
        &mut writer,
        process_name.as_str(),
        &mut last_sent_manifest_revision,
    )
    .await?;

    let mut cursor = SeqNo::ZERO;
    let mut last_sent_backtrace_id = None;
    let mut ticker = tokio::time::interval(Duration::from_millis(DASHBOARD_PUSH_INTERVAL_MS));
    ticker.set_missed_tick_behavior(MissedTickBehavior::Delay);

    loop {
        tokio::select! {
            _ = ticker.tick() => {
                let requested_from = cursor;
                let batch = pull_changes_since(cursor, DASHBOARD_PUSH_MAX_CHANGES);
                let cursor_shifted = batch.from_seq_no > requested_from || batch.next_seq_no > requested_from;
                if !batch.changes.is_empty() || batch.truncated || cursor_shifted {
                    let next = batch.next_seq_no;
                    flush_backtrace_records(
                        &mut writer,
                        process_name.as_str(),
                        &mut last_sent_manifest_revision,
                        &mut last_sent_backtrace_id,
                    )
                    .await?;
                    write_client_message(&mut writer, &ClientMessage::DeltaBatch(batch)).await?;
                    cursor = next.max(cursor);
                } else {
                    cursor = batch.next_seq_no.max(cursor);
                }
            }
            inbound = read_server_message(&mut reader) => {
                let Some(message) = inbound? else {
                    return Ok(());
                };
                match message {
                    ServerMessage::CutRequest(request) => {
                        flush_backtrace_records(
                            &mut writer,
                            process_name.as_str(),
                            &mut last_sent_manifest_revision,
                            &mut last_sent_backtrace_id,
                        )
                        .await?;
                        let ack = ack_cut(request.cut_id.clone());
                        write_client_message(&mut writer, &ClientMessage::CutAck(ack)).await?;
                    }
                    ServerMessage::SnapshotRequest(request) => {
                        flush_backtrace_records(
                            &mut writer,
                            process_name.as_str(),
                            &mut last_sent_manifest_revision,
                            &mut last_sent_backtrace_id,
                        )
                        .await?;
                        let frame = super::db::encode_snapshot_reply_frame(request.snapshot_id)?;
                        writer
                            .write_all(&frame)
                            .await
                            .map_err(|e| format!("write frame: {e}"))?;
                    }
                }
            }
        }
    }
}

// r[impl wire.backtrace-record]
async fn flush_backtrace_records(
    writer: &mut tokio::net::tcp::OwnedWriteHalf,
    process_name: &str,
    last_sent_manifest_revision: &mut u64,
    last_sent_backtrace_id: &mut Option<moire_trace_types::BacktraceId>,
) -> Result<(), String> {
    let records = super::backtrace_records_after(*last_sent_backtrace_id);
    send_handshake_if_manifest_changed(writer, process_name, last_sent_manifest_revision).await?;
    for record in records {
        let record_id = record.id;
        write_client_message(writer, &ClientMessage::BacktraceRecord(record)).await?;
        *last_sent_backtrace_id = Some(record_id);
    }
    Ok(())
}

async fn send_handshake_if_manifest_changed(
    writer: &mut tokio::net::tcp::OwnedWriteHalf,
    process_name: &str,
    last_sent_manifest_revision: &mut u64,
) -> Result<(), String> {
    let (revision, module_manifest) = super::module_manifest_snapshot();
    if revision == *last_sent_manifest_revision {
        return Ok(());
    }
    let handshake = ClientMessage::Handshake(moire_wire::Handshake {
        process_name: process_name.to_string(),
        pid: std::process::id(),
        args: std::env::args().collect(),
        env: std::env::vars()
            .map(|(key, value)| format!("{key}={value}"))
            .collect(),
        module_manifest,
    });
    write_client_message(writer, &handshake).await?;
    *last_sent_manifest_revision = revision;
    Ok(())
}

async fn write_client_message(
    writer: &mut tokio::net::tcp::OwnedWriteHalf,
    message: &ClientMessage,
) -> Result<(), String> {
    let frame = encode_client_message_default(message)
        .map_err(|e| format!("encode client message: {e}"))?;
    writer
        .write_all(&frame)
        .await
        .map_err(|e| format!("write frame: {e}"))?;
    Ok(())
}

async fn read_server_message(
    reader: &mut tokio::net::tcp::OwnedReadHalf,
) -> Result<Option<ServerMessage>, String> {
    let mut len_buf = [0u8; 4];
    if let Err(e) = reader.read_exact(&mut len_buf).await {
        if e.kind() == std::io::ErrorKind::UnexpectedEof {
            return Ok(None);
        }
        return Err(format!("read frame len: {e}"));
    }
    let payload_len = u32::from_be_bytes(len_buf) as usize;
    if payload_len > moire_wire::DEFAULT_MAX_FRAME_BYTES {
        return Err(format!("server frame too large: {payload_len}"));
    }
    let mut payload = vec![0u8; payload_len];
    reader
        .read_exact(&mut payload)
        .await
        .map_err(|e| format!("read frame payload: {e}"))?;
    let mut framed = Vec::with_capacity(4 + payload_len);
    framed.extend_from_slice(&len_buf);
    framed.extend_from_slice(&payload);
    let message = decode_server_message_default(&framed)
        .map_err(|e| format!("decode server message: {e}"))?;
    Ok(Some(message))
}
