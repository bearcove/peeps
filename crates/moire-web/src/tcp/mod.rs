use std::path::Path as FsPath;
use std::sync::Arc;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{Notify, mpsc};
use tracing::{debug, error, info, warn};

use crate::app::{AppState, ConnectedProcess, ConnectionId};
use crate::db::{
    backtrace_frames_for_store, into_stored_module_manifest, persist_backtrace_record,
    persist_connection_closed, persist_connection_module_manifest, persist_connection_upsert,
    persist_cut_ack, persist_delta_batch,
};
use moire_wire::{ClientMessage, decode_client_message_default, decode_protocol_magic};

pub async fn run_tcp_acceptor(listener: TcpListener, state: AppState) {
    loop {
        match listener.accept().await {
            Ok((stream, addr)) => {
                info!(%addr, "TCP connection accepted");
                let st = state.clone();
                tokio::spawn(async move {
                    if let Err(e) = handle_conn(stream, st).await {
                        error!(%addr, %e, "connection error");
                    }
                });
            }
            Err(e) => error!(%e, "TCP accept failed"),
        }
    }
}

async fn handle_conn(stream: TcpStream, state: AppState) -> Result<(), String> {
    let (mut reader, mut writer) = stream.into_split();
    let (msg_tx, mut msg_rx) = mpsc::channel::<Vec<u8>>(32);

    let conn_id = {
        let mut guard = state.inner.lock().await;
        let conn_id = guard.next_conn_id;
        guard.next_conn_id = conn_id.next();
        guard.connections.insert(
            conn_id,
            ConnectedProcess {
                process_name: format!("unknown-{conn_id}"),
                pid: 0,
                handshake_received: false,
                module_manifest: Vec::new(),
                tx: msg_tx,
            },
        );
        conn_id
    };
    if let Err(e) =
        persist_connection_upsert(state.db.clone(), conn_id, format!("unknown-{conn_id}"), 0).await
    {
        warn!(conn_id = %conn_id, %e, "failed to persist connection row");
    }

    let writer_handle = tokio::spawn(async move {
        while let Some(frame) = msg_rx.recv().await {
            if writer.write_all(&frame).await.is_err() {
                break;
            }
        }
    });

    let read_result = read_messages(conn_id, &mut reader, &state).await;

    let to_notify: Vec<Arc<Notify>> = {
        let mut guard = state.inner.lock().await;
        guard.connections.remove(&conn_id);
        for cut in guard.cuts.values_mut() {
            cut.pending_conn_ids.remove(&conn_id);
            cut.acks.remove(&conn_id);
        }
        guard
            .pending_snapshots
            .values_mut()
            .filter_map(|pending| {
                if pending.pending_conn_ids.remove(&conn_id) && pending.pending_conn_ids.is_empty()
                {
                    Some(pending.notify.clone())
                } else {
                    None
                }
            })
            .collect()
    };
    for notify in to_notify {
        notify.notify_one();
    }
    if let Err(e) = persist_connection_closed(state.db.clone(), conn_id).await {
        warn!(conn_id = %conn_id, %e, "failed to persist connection close");
    }

    writer_handle.abort();
    read_result
}

async fn read_messages(
    conn_id: ConnectionId,
    reader: &mut tokio::net::tcp::OwnedReadHalf,
    state: &AppState,
) -> Result<(), String> {
    let mut magic = [0u8; 4];
    reader
        .read_exact(&mut magic)
        .await
        .map_err(|e| format!("read protocol magic: {e}"))?;
    decode_protocol_magic(magic).map_err(|e| format!("invalid protocol magic: {e}"))?;

    loop {
        let mut len_buf = [0u8; 4];
        if let Err(e) = reader.read_exact(&mut len_buf).await {
            if e.kind() == std::io::ErrorKind::UnexpectedEof {
                debug!(conn_id = %conn_id, "connection closed (EOF)");
                return Ok(());
            }
            return Err(format!("read frame len: {e}"));
        }

        let payload_len = u32::from_be_bytes(len_buf) as usize;
        if payload_len > moire_wire::DEFAULT_MAX_FRAME_BYTES {
            return Err(format!("frame too large: {payload_len}"));
        }

        let mut payload = vec![0u8; payload_len];
        reader
            .read_exact(&mut payload)
            .await
            .map_err(|e| format!("read frame payload: {e}"))?;

        let mut framed = Vec::with_capacity(4 + payload_len);
        framed.extend_from_slice(&len_buf);
        framed.extend_from_slice(&payload);
        let message = decode_client_message_default(&framed)
            .map_err(|e| format!("decode client message: {e}"))?;

        match message {
            ClientMessage::Handshake(handshake) => {
                validate_handshake(&handshake)
                    .map_err(|e| format!("reject handshake for conn {conn_id}: {e}"))?;
                let process_name = handshake.process_name.to_string();
                let pid = handshake.pid;
                let module_manifest_entries = handshake.module_manifest.len();
                let stored_manifest = into_stored_module_manifest(handshake.module_manifest);
                let mut guard = state.inner.lock().await;
                if let Some(conn) = guard.connections.get_mut(&conn_id) {
                    conn.process_name = process_name.clone();
                    conn.pid = pid;
                    conn.handshake_received = true;
                    conn.module_manifest = stored_manifest.clone();
                }
                drop(guard);
                if let Err(e) =
                    persist_connection_upsert(state.db.clone(), conn_id, process_name.clone(), pid)
                        .await
                {
                    warn!(conn_id = %conn_id, %e, "failed to persist handshake");
                }
                if let Err(e) =
                    persist_connection_module_manifest(state.db.clone(), conn_id, stored_manifest)
                        .await
                {
                    warn!(conn_id = %conn_id, %e, "failed to persist module manifest");
                }
                info!(
                    conn_id = %conn_id,
                    process_name, pid, module_manifest_entries, "handshake accepted"
                );
            }
            ClientMessage::SnapshotReply(reply) => {
                info!(
                    conn_id = %conn_id,
                    snapshot_id = reply.snapshot_id,
                    has_snapshot = reply.snapshot.is_some(),
                    "received snapshot reply"
                );
                let notify_opt = {
                    let mut guard = state.inner.lock().await;
                    if let Some(pending) = guard.pending_snapshots.get_mut(&reply.snapshot_id) {
                        pending.pending_conn_ids.remove(&conn_id);
                        pending.replies.insert(conn_id, reply);
                        if pending.pending_conn_ids.is_empty() {
                            Some(pending.notify.clone())
                        } else {
                            None
                        }
                    } else {
                        debug!(
                            conn_id = %conn_id,
                            snapshot_id = reply.snapshot_id,
                            "snapshot reply for unknown id"
                        );
                        None
                    }
                };
                if let Some(notify) = notify_opt {
                    notify.notify_one();
                }
            }
            ClientMessage::DeltaBatch(batch) => {
                if let Err(e) = persist_delta_batch(state.db.clone(), conn_id, batch).await {
                    warn!(conn_id = %conn_id, %e, "failed to persist delta batch");
                }
            }
            ClientMessage::CutAck(ack) => {
                let cut_id_text = ack.cut_id.as_str().to_owned();
                let cursor_stream_id = ack.cursor.stream_id.0.to_string();
                let cursor_next_seq_no = ack.cursor.next_seq_no.0;
                let cut_id = ack.cut_id.clone();
                let mut guard = state.inner.lock().await;
                if let Some(cut) = guard.cuts.get_mut(&cut_id) {
                    cut.pending_conn_ids.remove(&conn_id);
                    cut.acks.insert(conn_id, ack);
                    info!(
                        conn_id = %conn_id,
                        cut_id = %cut_id,
                        pending_connections = cut.pending_conn_ids.len(),
                        acked_connections = cut.acks.len(),
                        "received cut ack"
                    );
                } else {
                    warn!(
                        conn_id = %conn_id,
                        cut_id = %cut_id,
                        "received cut ack for unknown cut"
                    );
                }
                drop(guard);
                if let Err(e) = persist_cut_ack(
                    state.db.clone(),
                    cut_id_text,
                    conn_id,
                    cursor_stream_id,
                    cursor_next_seq_no,
                )
                .await
                {
                    warn!(conn_id = %conn_id, %e, "failed to persist cut ack");
                }
            }
            ClientMessage::Error(msg) => {
                warn!(
                    conn_id = %conn_id,
                    process_name = %msg.process_name,
                    stage = %msg.stage,
                    error = %msg.error,
                    "client reported protocol/runtime error"
                );
            }
            ClientMessage::BacktraceRecord(record) => {
                let (handshake_received, manifest) = {
                    let guard = state.inner.lock().await;
                    guard
                        .connections
                        .get(&conn_id)
                        .map(|conn| (conn.handshake_received, conn.module_manifest.clone()))
                        .ok_or_else(|| {
                            format!(
                                "invariant violated: unknown connection {} for backtrace {}",
                                conn_id, record.id
                            )
                        })?
                };
                if !handshake_received {
                    return Err(format!(
                        "protocol violation: received backtrace {} before handshake on conn {}",
                        record.id, conn_id
                    ));
                }
                let backtrace_id = record.id;
                let frames = backtrace_frames_for_store(&manifest, &record)?;
                let inserted =
                    persist_backtrace_record(state.db.clone(), conn_id, backtrace_id, frames)
                        .await?;
                if !inserted {
                    debug!(
                        conn_id = %conn_id,
                        backtrace_id = %backtrace_id,
                        "backtrace already existed in storage"
                    );
                }
            }
        }
    }
}

fn validate_handshake(handshake: &moire_wire::Handshake) -> Result<(), String> {
    if handshake.process_name.trim().is_empty() {
        return Err("process_name must be non-empty".to_string());
    }

    for (index, module) in handshake.module_manifest.iter().enumerate() {
        if module.module_path.trim().is_empty() {
            return Err(format!(
                "module_manifest[{index}].module_path must be non-empty"
            ));
        }
        if !FsPath::new(module.module_path.as_str()).is_absolute() {
            return Err(format!(
                "module_manifest[{index}].module_path must be absolute"
            ));
        }
        if module.runtime_base.get() == 0 {
            return Err(format!(
                "module_manifest[{index}].runtime_base must be non-zero"
            ));
        }
        if module.arch.trim().is_empty() {
            return Err(format!("module_manifest[{index}].arch must be non-empty"));
        }
        match &module.identity {
            moire_wire::ModuleIdentity::BuildId(build_id) => {
                if build_id.trim().is_empty() {
                    return Err(format!(
                        "module_manifest[{index}].identity.build_id must be non-empty"
                    ));
                }
            }
            moire_wire::ModuleIdentity::DebugId(debug_id) => {
                if debug_id.trim().is_empty() {
                    return Err(format!(
                        "module_manifest[{index}].identity.debug_id must be non-empty"
                    ));
                }
            }
        }
    }
    Ok(())
}
