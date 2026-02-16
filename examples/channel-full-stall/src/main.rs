use std::time::Duration;

#[tokio::main]
async fn main() {
    peeps::init("example-channel-full-stall");

    let (tx, mut rx) = peeps::channel::<u32>("demo.work_queue", 16);

    peeps::spawn_tracked("stalled_receiver", async move {
        println!("receiver started but is intentionally not draining the queue");
        peeps::peep!(
            tokio::time::sleep(Duration::from_secs(3600)),
            "receiver.simulated_hang"
        )
        .await;
        let _ = rx.recv().await;
    });

    peeps::spawn_tracked("bounded_sender", async move {
        for i in 0_u32..16 {
            peeps::peep!(
                tx.send(i),
                "queue.send.prefill",
                {
                    "queue.name" => "demo.work_queue",
                    "item" => i,
                    "phase" => "prefill"
                }
            )
            .await
            .expect("channel is open while pre-filling buffer");
            println!("sent prefill item {i}");
        }

        println!("attempting 17th send; this should block because capacity is 16 and receiver is stalled");

        peeps::peep!(
            tx.send(16),
            "queue.send.blocked",
            {
                "queue.name" => "demo.work_queue",
                "item" => 16_u32,
                "phase" => "blocked"
            }
        )
        .await
        .expect("send unexpectedly unblocked");
    });

    println!("example running. open peeps-web and inspect demo.work_queue");
    println!("press Ctrl+C to exit");
    let _ = tokio::signal::ctrl_c().await;
}
