use std::io;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;

use roam_session::{accept_framed, HandshakeConfig, MessageTransport, NoDispatcher};
use roam_wire::Message;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::process::{Child, Command};

struct TcpMessageTransport {
    stream: tokio::net::TcpStream,
    last_decoded: Vec<u8>,
}

impl TcpMessageTransport {
    fn new(stream: tokio::net::TcpStream) -> Self {
        Self {
            stream,
            last_decoded: Vec::new(),
        }
    }

    async fn recv_frame(&mut self) -> io::Result<Option<Vec<u8>>> {
        let mut len_buf = [0_u8; 4];
        match self.stream.read_exact(&mut len_buf).await {
            Ok(_) => {}
            Err(err) if err.kind() == io::ErrorKind::UnexpectedEof => return Ok(None),
            Err(err) => return Err(err),
        }

        let frame_len = u32::from_le_bytes(len_buf) as usize;
        let mut payload = vec![0_u8; frame_len];
        self.stream.read_exact(&mut payload).await?;
        Ok(Some(payload))
    }
}

impl MessageTransport for TcpMessageTransport {
    async fn send(&mut self, msg: &Message) -> io::Result<()> {
        let payload = facet_postcard::to_vec(msg)
            .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err.to_string()))?;
        let frame_len = u32::try_from(payload.len()).map_err(|_| {
            io::Error::new(
                io::ErrorKind::InvalidInput,
                "message too large for u32 frame length",
            )
        })?;

        self.stream.write_all(&frame_len.to_le_bytes()).await?;
        self.stream.write_all(&payload).await?;
        self.stream.flush().await
    }

    async fn recv_timeout(&mut self, timeout: Duration) -> io::Result<Option<Message>> {
        match tokio::time::timeout(timeout, self.recv()).await {
            Ok(result) => result,
            Err(_) => Ok(None),
        }
    }

    async fn recv(&mut self) -> io::Result<Option<Message>> {
        let Some(payload) = self.recv_frame().await? else {
            return Ok(None);
        };

        self.last_decoded = payload.clone();
        let msg = facet_postcard::from_slice(&payload)
            .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err.to_string()))?;
        Ok(Some(msg))
    }

    fn last_decoded(&self) -> &[u8] {
        &self.last_decoded
    }
}

fn swift_package_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("swift")
}

fn spawn_swift_peer(peer_addr: &str) -> io::Result<Child> {
    let mut cmd = Command::new("swift");
    cmd.arg("run")
        .arg("--package-path")
        .arg(swift_package_path())
        .arg("rust_swift_peer")
        .env("PEER_ADDR", peer_addr)
        .stdin(Stdio::null())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit());

    cmd.spawn()
}

#[tokio::main]
async fn main() {
    peeps::init("example-roam-rust-swift-stuck-request.rust");

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("failed to bind listener");
    let addr = listener
        .local_addr()
        .expect("failed to get listener local_addr");

    println!("listening for swift peer on {addr}");

    let mut swift_child = spawn_swift_peer(&addr.to_string())
        .expect("failed to spawn swift runtime peer (requires `swift` toolchain)");

    let (stream, peer_addr) = listener
        .accept()
        .await
        .expect("failed to accept swift peer connection");
    println!("swift peer connected from {peer_addr}");

    let mut config = HandshakeConfig::default();
    config.name = Some("rust-host".to_string());

    let transport = TcpMessageTransport::new(stream);
    let (handle, _incoming, driver) = accept_framed(transport, config, NoDispatcher)
        .await
        .expect("roam handshake with swift peer should succeed");

    peeps::spawn_tracked!("roam.rust_host_driver", async move {
        let _ = driver.run().await;
    });

    let request_handle = handle.clone();
    peeps::spawn_tracked!("rust.calls.swift_noop", async move {
        let _ = peeps::peep!(
            request_handle.call_raw(0xfeed_f00d, "swift.noop.stall", Vec::new()),
            "rpc.call.swift.noop.stall"
        )
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
}
