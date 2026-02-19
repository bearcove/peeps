use std::time::Duration;

use crate::peeps::prelude::*;

const SOURCE_LEFT: peeps::SourceLeft =
    peeps::SourceLeft::new(env!("CARGO_MANIFEST_DIR"), env!("CARGO_PKG_NAME"));

#[track_caller]
fn source() -> peeps::Source {
    SOURCE_LEFT.resolve()
}

pub async fn run() -> Result<(), String> {
    peeps::__init_from_macro();

    let (tx, mut rx) = peeps::channel("demo.work_queue", 16, peeps::SourceRight::caller());
    let (_idle_tx, mut idle_rx) =
        peeps::channel("demo.idle_queue", 1, peeps::SourceRight::caller());

    peeps::spawn_tracked(
        "stalled_receiver",
        async move {
            println!("receiver started but is intentionally not draining the queue");
            tokio::time::sleep(Duration::from_secs(3600)).await;
            let _ = rx.recv().await;
        },
        source(),
    );

    peeps::spawn_tracked(
        "blocked_receiver",
        async move {
            println!(
                "blocked_receiver waits on demo.idle_queue.recv() forever (no sender activity)"
            );
            let _: Option<u32> = idle_rx.recv().await;
        },
        source(),
    );

    peeps::spawn_tracked(
        "bounded_sender",
        async move {
            for i in 0_u32..16 {
                tx.send(i)
                    .await
                    .expect("channel is open while pre-filling buffer");
                println!("sent prefill item {i}");
            }

            println!("attempting 17th send; this should block because capacity is 16 and receiver is stalled");

            tx.send(16).await.expect("send unexpectedly unblocked");
        },
        source(),
    );

    println!("example running. open peeps-web and inspect demo.work_queue");
    println!("press Ctrl+C to exit");
    tokio::signal::ctrl_c()
        .await
        .map_err(|e| format!("failed waiting for Ctrl+C: {e}"))?;
    Ok(())
}
