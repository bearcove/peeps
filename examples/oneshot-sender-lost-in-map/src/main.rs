use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

type RequestId = u64;
type ResponsePayload = String;
type PendingMap = Arc<peeps::Mutex<HashMap<RequestId, peeps::OneshotSender<ResponsePayload>>>>;

fn storage_key_for_request(request_id: RequestId) -> RequestId {
    request_id + 1
}

fn lookup_key_for_response(response_id: RequestId) -> RequestId {
    response_id
}

#[tokio::main]
async fn main() {
    peeps::init("example-oneshot-sender-lost-in-map");

    let pending_by_request_id: PendingMap = Arc::new(peeps::mutex!(
        "demo.pending_oneshot_senders",
        HashMap::new(),
    ));
    let (response_bus_tx, mut response_bus_rx) = peeps::channel!("demo.response_bus", 4);

    let pending_for_request = Arc::clone(&pending_by_request_id);
    peeps::spawn_tracked!("client.request_42.await_response", async move {
        let request_id = 42_u64;
        let (tx, rx) = peeps::oneshot!("demo.request_42.response");

        let storage_key = storage_key_for_request(request_id);
        pending_for_request.lock().insert(storage_key, tx);
        println!(
            "inserted sender for request {request_id} under wrong key {storage_key}; receiver now waits"
        );

        peeps::peep!(rx.recv(), "request_42.await_response.blocked")
            .await
            .expect("request unexpectedly completed");
    });

    let bus_tx_for_network = response_bus_tx.clone();
    peeps::spawn_tracked!("network.inject_single_response", async move {
        tokio::time::sleep(Duration::from_millis(200)).await;
        println!("network delivered one response for request 42");
        bus_tx_for_network
            .send((42_u64, String::from("ok")))
            .await
            .expect("response bus unexpectedly closed");
    });

    let pending_for_router = Arc::clone(&pending_by_request_id);
    peeps::spawn_tracked!("router.match_response_to_pending_request", async move {
        loop {
            let Some((request_id, payload)) =
                peeps::peep!(response_bus_rx.recv(), "response_bus.recv").await
            else {
                return;
            };

            let lookup_key = lookup_key_for_response(request_id);
            let maybe_sender = pending_for_router.lock().remove(&lookup_key);

            if let Some(sender) = maybe_sender {
                sender
                    .send(payload)
                    .expect("oneshot receiver unexpectedly dropped");
                println!("response for request {request_id} routed successfully");
                continue;
            }

            let known_keys: Vec<_> = pending_for_router.lock().keys().copied().collect();
            println!(
                "router miss: looked for key {lookup_key}, map has {known_keys:?}; sender stays alive but unreachable"
            );
        }
    });

    println!("example running. open peeps-web and inspect demo.request_42.response");
    println!("press Ctrl+C to exit");
    let _ = tokio::signal::ctrl_c().await;
}
