use std::path::{Path, PathBuf};
use std::process::Stdio;

use crate::peeps::prelude::*;
use roam_stream::{accept, HandshakeConfig, NoDispatcher};
use tokio::process::{Child, Command};

fn swift_package_path(workspace_root: &Path) -> PathBuf {
    workspace_root.join("crates/peeps-examples/swift/roam-rust-swift-stuck-request")
}

fn spawn_swift_peer(workspace_root: &Path, peer_addr: &str) -> std::io::Result<Child> {
    let mut cmd = Command::new("swift");
    cmd.arg("run")
        .arg("--package-path")
        .arg(swift_package_path(workspace_root))
        .arg("rust_swift_peer")
        .env("PEER_ADDR", peer_addr)
        .stdin(Stdio::null())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit());

    ur_taking_me_with_you::spawn_dying_with_parent_async(cmd)
}

pub async fn run(workspace_root: &Path) -> Result<(), String> {
    peeps::__init_from_macro();

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .map_err(|e| format!("failed to bind listener: {e}"))?;
    let addr = listener
        .local_addr()
        .map_err(|e| format!("failed to get listener local_addr: {e}"))?;

    println!("listening for swift peer on {addr}");

    let mut swift_child = spawn_swift_peer(workspace_root, &addr.to_string()).map_err(|e| {
        format!("failed to spawn swift runtime peer (requires `swift` toolchain): {e}")
    })?;

    let (stream, peer_addr) = listener
        .accept()
        .await
        .map_err(|e| format!("failed to accept swift peer connection: {e}"))?;
    println!("swift peer connected from {peer_addr}");

    let mut config = HandshakeConfig::default();
    config.name = Some("rust-host".to_string());

    let (handle, _incoming, driver) = accept(stream, config, NoDispatcher)
        .await
        .map_err(|e| format!("roam handshake with swift peer should succeed: {e}"))?;

    crate::peeps::spawn_tracked("roam.rust_host_driver", async move {
        let _ = driver.run().await;
    });

    let request_handle = handle.clone();
    crate::peeps::spawn_tracked("rust.calls.swift_noop", async move {
        let _ = request_handle
            .call_raw(0xfeed_f00d, "swift.noop.stall", Vec::new())
            .tracked("rpc.call.swift.noop.stall")
            .await;
    });

    println!("example running. rust issues one RPC call that swift intentionally never answers.");
    println!("open peeps-web and inspect request/connection wait edges across this process.");
    println!("press Ctrl+C to exit");

    tokio::select! {
        _ = tokio::signal::ctrl_c() => {
            println!("received Ctrl+C, shutting down");
        }
        status = swift_child.wait() => {
            println!("swift peer exited early: {status:?}");
        }
    }

    let _ = swift_child.kill().await;
    let _ = swift_child.wait().await;
    Ok(())
}
