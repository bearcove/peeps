use std::io;
use std::time::Duration;

use roam::service;
use roam_session::{accept_framed, initiate_framed, HandshakeConfig, MessageTransport, NoDispatcher};
use roam_wire::Message;
use tokio::sync::mpsc;

#[service]
trait DemoRpc {
    async fn sleepy_forever(&self, span_id: String) -> String;
}

#[derive(Clone, Default)]
struct DemoService;

impl DemoRpc for DemoService {
    async fn sleepy_forever(&self, _cx: &roam::Context, span_id: String) -> String {
        let request_id = peeps::types::canonical_id::request_from_span_id(&span_id);
        let response_id = format!("response:{span_id}");

        peeps::rpc_response_event!(
            &response_id,
            "DemoRpc.sleepy_forever",
            parent = &request_id,
            {
                "rpc.connection" => "in_memory",
                "request.id" => span_id.as_str(),
                "request.method" => "sleepy_forever",
                "rpc.state" => "handler_started"
            }
        );

        peeps::peep!(
            async {
                loop {
                    tokio::time::sleep(Duration::from_secs(60)).await;
                }
            },
            "rpc.handler.sleep_forever",
            {
                "rpc.connection" => "in_memory",
                "request.id" => span_id.as_str(),
                "request.method" => "sleepy_forever"
            }
        )
        .await;

        unreachable!();
    }
}

struct InMemoryTransport {
    tx: mpsc::Sender<Message>,
    rx: mpsc::Receiver<Message>,
    last_decoded: Vec<u8>,
}

fn in_memory_transport_pair(buffer: usize) -> (InMemoryTransport, InMemoryTransport) {
    let (a_to_b_tx, a_to_b_rx) = mpsc::channel(buffer);
    let (b_to_a_tx, b_to_a_rx) = mpsc::channel(buffer);

    (
        InMemoryTransport {
            tx: a_to_b_tx,
            rx: b_to_a_rx,
            last_decoded: Vec::new(),
        },
        InMemoryTransport {
            tx: b_to_a_tx,
            rx: a_to_b_rx,
            last_decoded: Vec::new(),
        },
    )
}

impl MessageTransport for InMemoryTransport {
    async fn send(&mut self, msg: &Message) -> io::Result<()> {
        self.tx
            .send(msg.clone())
            .await
            .map_err(|_| io::Error::new(io::ErrorKind::BrokenPipe, "peer disconnected"))
    }

    async fn recv_timeout(&mut self, timeout: Duration) -> io::Result<Option<Message>> {
        match tokio::time::timeout(timeout, self.rx.recv()).await {
            Ok(msg) => Ok(msg),
            Err(_) => Ok(None),
        }
    }

    async fn recv(&mut self) -> io::Result<Option<Message>> {
        Ok(self.rx.recv().await)
    }

    fn last_decoded(&self) -> &[u8] {
        &self.last_decoded
    }
}

#[tokio::main]
async fn main() {
    peeps::init("example-roam-rpc-stuck-request");

    let (client_transport, server_transport) = in_memory_transport_pair(128);
    let dispatcher = DemoRpcDispatcher::new(DemoService);

    let client_fut = initiate_framed(client_transport, HandshakeConfig::default(), NoDispatcher);
    let server_fut = accept_framed(server_transport, HandshakeConfig::default(), dispatcher);

    let (client_setup, server_setup) = tokio::try_join!(client_fut, server_fut)
        .expect("in-memory roam connection setup should succeed");

    let (client_handle, _incoming_client, client_driver) = client_setup;
    let (_server_handle, _incoming_server, server_driver) = server_setup;

    peeps::spawn_tracked("roam.client_driver", async move {
        let _ = client_driver.run().await;
    });

    peeps::spawn_tracked("roam.server_driver", async move {
        let _ = server_driver.run().await;
    });

    let client = DemoRpcClient::new(client_handle);

    peeps::spawn_tracked("roam.client.request_task", async move {
        let span_id = ulid::Ulid::new().to_string();
        let request_id = peeps::types::canonical_id::request_from_span_id(&span_id);

        peeps::rpc_request_event!(&request_id, "DemoRpc.sleepy_forever", {
            "rpc.connection" => "in_memory",
            "request.id" => span_id.as_str(),
            "request.method" => "sleepy_forever",
            "rpc.state" => "sent"
        });

        peeps::peep!(
            client.sleepy_forever(span_id.clone()),
            "rpc.client.await_response",
            {
                "rpc.connection" => "in_memory",
                "request.id" => span_id.as_str(),
                "request.method" => "sleepy_forever"
            }
        )
        .await
        .expect("request unexpectedly completed");
    });

    println!("example running: one roam RPC request is intentionally stuck forever");
    println!("press Ctrl+C to exit");
    let _ = tokio::signal::ctrl_c().await;
}
