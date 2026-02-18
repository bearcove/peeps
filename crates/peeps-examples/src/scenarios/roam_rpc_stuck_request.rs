use std::io;
use std::process::Stdio;
use std::time::Duration;

use roam::service;
use roam_stream::{accept, connect, Connector, HandshakeConfig, NoDispatcher};
use tokio::net::TcpStream;
use tokio::process::{Child, Command};

#[service]
trait DemoRpc {
    async fn sleepy_forever(&self) -> String;
}

#[derive(Clone, Default)]
struct DemoService;

impl DemoRpc for DemoService {
    async fn sleepy_forever(&self, _cx: &roam::Context) -> String {
        loop {
            tokio::time::sleep(Duration::from_secs(60)).await;
        }
    }
}

struct TcpConnector {
    addr: String,
}

impl Connector for TcpConnector {
    type Transport = TcpStream;

    async fn connect(&self) -> io::Result<TcpStream> {
        TcpStream::connect(&self.addr).await
    }
}

pub async fn run() -> Result<(), String> {
    peeps::init!();

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .map_err(|e| format!("server failed to bind tcp listener: {e}"))?;
    let bound_addr = listener
        .local_addr()
        .map_err(|e| format!("failed to get local addr: {e}"))?;
    println!("server listening on {bound_addr}");

    let mut client_child = spawn_client_process(&bound_addr.to_string())?;

    let (stream, peer_addr) = listener
        .accept()
        .await
        .map_err(|e| format!("server failed to accept client connection: {e}"))?;

    println!("client connected from {peer_addr}");

    let config = HandshakeConfig {
        name: Some("stuck-server".to_string()),
        ..Default::default()
    };

    let dispatcher = DemoRpcDispatcher::new(DemoService);
    let (_handle, _incoming, driver) = accept(stream, config, dispatcher)
        .await
        .map_err(|e| format!("server handshake should succeed: {e}"))?;

    peeps::spawn_tracked!("roam.server_driver", async move {
        let _ = driver.run().await;
    });

    println!("server ready: requests to sleepy_forever will stall forever");
    println!("press Ctrl+C to exit");
    tokio::select! {
        _ = tokio::signal::ctrl_c() => {
            println!("received Ctrl+C, shutting down");
        }
        status = client_child.wait() => {
            println!("client process exited early: {status:?}");
        }
    }

    let _ = client_child.kill().await;
    let _ = client_child.wait().await;
    Ok(())
}

pub async fn run_client_process(addr: String) -> Result<(), String> {
    peeps::init!();
    run_client(addr).await
}

fn spawn_client_process(addr: &str) -> Result<Child, String> {
    let exe = std::env::current_exe().map_err(|e| format!("failed to resolve current exe: {e}"))?;
    let mut cmd = Command::new(exe);
    cmd.arg("--no-web")
        .arg("roam-rpc-stuck-request-client")
        .arg("--peer-addr")
        .arg(addr)
        .env(crate::EXAMPLE_CHILD_MODE_ENV, "1")
        .stdin(Stdio::null())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit());

    ur_taking_me_with_you::spawn_dying_with_parent_async(cmd)
        .map_err(|e| format!("failed to spawn roam rpc client process: {e}"))
}

async fn run_client(addr: String) -> Result<(), String> {
    let mut config = HandshakeConfig::default();
    config.name = Some("stuck-client".to_string());

    let connector = TcpConnector { addr };
    let client_transport = connect(connector, config, NoDispatcher);

    let client = DemoRpcClient::new(client_transport);
    println!("client: sent one sleepy_forever RPC request (intentionally stuck)");
    let _ = peeps::peep!(client.sleepy_forever(), "roam.client.request_task")
        .await
        .map_err(|e| format!("client sleepy_forever request failed: {e}"))?;
    Err("client request unexpectedly completed".to_string())
}
