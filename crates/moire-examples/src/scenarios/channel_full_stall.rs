use std::time::Duration;

use moire::{sync::mpsc, task::spawn};

pub async fn run() -> Result<(), String> {
    let (tx, mut rx) = mpsc::channel("demo.work_queue", 16);
    let (_idle_tx, mut idle_rx) = mpsc::channel("demo.idle_queue", 1);

    spawn(async move {
        println!("receiver started but is intentionally not draining the queue");
        tokio::time::sleep(Duration::from_secs(3600)).await;
        let _ = rx.recv().await;
    })
    .named("stalled_receiver");

    spawn(async move {
        println!("blocked_receiver waits on demo.idle_queue.recv() forever (no sender activity)");
        let _: Option<u32> = idle_rx.recv().await;
    })
    .named("blocked_receiver");

    spawn(async move {
        for i in 0_u32..16 {
            tx.send(i)
                .await
                .expect("channel is open while pre-filling buffer");
            println!("sent prefill item {i}");
        }

        println!(
            "attempting 17th send; this should block because capacity is 16 and receiver is stalled"
        );

        tx.send(16).await.expect("send unexpectedly unblocked");
    })
    .named("bounded_sender");

    println!("example running. open moire-web and inspect demo.work_queue");
    println!("press Ctrl+C to exit");
    tokio::signal::ctrl_c()
        .await
        .map_err(|e| format!("failed waiting for Ctrl+C: {e}"))?;
    Ok(())
}
